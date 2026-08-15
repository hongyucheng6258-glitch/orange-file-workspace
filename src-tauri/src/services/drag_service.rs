//! Windows 拖拽支持。
//!
//! 1. 拖入：wry 0.55.x 在 Windows 上存在 IDropTarget 注册竞态 —— webview 创建时即枚举
//!    子窗口并注册 DropTarget，但 dev 模式（页面走 localhost）下 WebView2 子窗口树形成
//!    较晚，注册落到了错误的 HWND 上，导致拖入文件无响应、光标显示禁止。
//!    这里在应用启动后延迟重新枚举子窗口并注册我们自己的 IDropTarget，覆盖 wry 的
//!    错误注册，并将事件以 `tauri://drag-*` 的格式转发给前端，与 Tauri 内置事件兼容。
//! 2. 拖出：通过 OLE `DoDragDrop` 将资源物理路径从应用窗口拖出（复制/移动到
//!    资源管理器等外部目标）。

use std::{
    cell::UnsafeCell,
    ffi::OsString,
    mem,
    os::windows::ffi::OsStringExt,
    path::PathBuf,
    ptr,
    rc::Rc,
    thread,
    time::Duration,
};

use tauri::{AppHandle, Emitter, Manager};
use windows::{
    core::{implement, BOOL, HRESULT, Ref, Result as WinResult},
    Win32::{
        Foundation::{DRAGDROP_E_INVALIDHWND, HGLOBAL, HWND, LPARAM, POINT, POINTL},
        Graphics::Gdi::ScreenToClient,
        System::{
            Com::{
                CoInitializeEx, CoUninitialize, IAdviseSink, IDataObject, IDataObject_Impl,
                IEnumFORMATETC, IEnumSTATDATA, COINIT_APARTMENTTHREADED, DVASPECT_CONTENT,
                FORMATETC, STGMEDIUM, TYMED_HGLOBAL,
            },
            Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT},
            Ole::{
                DoDragDrop, IDropSource, IDropSource_Impl, IDropTarget, IDropTarget_Impl,
                RegisterDragDrop, RevokeDragDrop, CF_HDROP, DROPEFFECT, DROPEFFECT_COPY,
                DROPEFFECT_MOVE, DROPEFFECT_NONE,
            },
            SystemServices::MODIFIERKEYS_FLAGS,
        },
        UI::{
            Shell::{DragFinish, DragQueryFileW, DROPFILES, HDROP},
            WindowsAndMessaging::EnumChildWindows,
        },
    },
};

const E_NOTIMPL: HRESULT = HRESULT(0x8000_4001u32 as _);
const MK_LBUTTON: u32 = 0x0001;
const MK_RBUTTON: u32 = 0x0002;
const MK_MBUTTON: u32 = 0x0010;
const DRAGDROP_S_CANCEL: HRESULT = HRESULT(0x0004_0101);
const DRAGDROP_S_DROP: HRESULT = HRESULT(0x0004_0100);
const DRAGDROP_S_USEDEFAULTCURSORS: HRESULT = HRESULT(0x0004_0102);
const RPC_E_CHANGED_MODE: HRESULT = HRESULT(0x8001_0106u32 as _);

/// 安装拖入支持：启动后延迟注册 DropTarget，修复 wry dev 模式竞态。
pub fn install_drop_target(app: AppHandle) {
    #[cfg(target_os = "windows")]
    thread::spawn(move || {
        // dev 模式页面加载慢，分两次注册确保子窗口树已稳定。
        for delay_secs in [2u64, 4] {
            thread::sleep(Duration::from_secs(delay_secs));
            if let Err(e) = register_drop_targets(&app) {
                eprintln!("drag-drop: register failed: {e}");
            }
        }
    });
    #[cfg(not(target_os = "windows"))]
    let _ = app;
}

#[cfg(target_os = "windows")]
fn register_drop_targets(app: &AppHandle) -> WinResult<()> {
    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };
    let Ok(hwnd) = window.hwnd() else {
        return Ok(());
    };
    let forwarder = Rc::new(EventForwarder { app: app.clone() });
    let mut registration = DropTargetRegistration::default();
    let mut callback = |child: HWND| registration.inject(child, forwarder.clone());
    let mut trait_obj: &mut dyn FnMut(HWND) -> bool = &mut callback;
    let closure_ptr: *mut std::os::raw::c_void =
        unsafe { mem::transmute(&mut trait_obj) };
    let lparam = LPARAM(closure_ptr as _);

    unsafe extern "system" fn enumerate_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let closure = &mut *(lparam.0 as *mut std::os::raw::c_void
            as *mut &mut dyn FnMut(HWND) -> bool);
        closure(hwnd).into()
    }

    let _ = unsafe { EnumChildWindows(Some(hwnd), Some(enumerate_callback), lparam) };
    eprintln!("drag-drop: registered {} drop target(s)", registration.targets.len());
    Ok(())
}

/// 拖入事件转发器：把拖拽事件以 Tauri 内置事件格式广播给前端。
#[cfg(target_os = "windows")]
struct EventForwarder {
    app: AppHandle,
}

#[cfg(target_os = "windows")]
impl EventForwarder {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        let _ = self.app.emit(event, payload);
    }
}

/// 收集注册成功的 DropTarget 引用，避免被释放导致注册失效。
#[cfg(target_os = "windows")]
#[derive(Default)]
struct DropTargetRegistration {
    targets: Vec<IDropTarget>,
}

#[cfg(target_os = "windows")]
impl DropTargetRegistration {
    fn inject(&mut self, hwnd: HWND, forwarder: Rc<EventForwarder>) -> bool {
        let target: IDropTarget = DropTarget::new(hwnd, forwarder).into();
        if unsafe { RevokeDragDrop(hwnd) } != Err(DRAGDROP_E_INVALIDHWND.into())
            && unsafe { RegisterDragDrop(hwnd, &target) }.is_ok()
        {
            self.targets.push(target);
        }
        true
    }
}

/// 拖入接收者：解析 CF_HDROP 并广播事件。
#[cfg(target_os = "windows")]
#[implement(IDropTarget)]
struct DropTarget {
    hwnd: HWND,
    forwarder: Rc<EventForwarder>,
    cursor_effect: UnsafeCell<DROPEFFECT>,
    enter_is_valid: UnsafeCell<bool>,
}

#[cfg(target_os = "windows")]
impl DropTarget {
    fn new(hwnd: HWND, forwarder: Rc<EventForwarder>) -> Self {
        Self {
            hwnd,
            forwarder,
            cursor_effect: DROPEFFECT_NONE.into(),
            enter_is_valid: false.into(),
        }
    }

    unsafe fn iterate_filenames<F>(data_obj: Ref<'_, IDataObject>, mut callback: F) -> Option<HDROP>
    where
        F: FnMut(PathBuf),
    {
        let format = FORMATETC {
            cfFormat: CF_HDROP.0,
            ptd: ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
        };
        match data_obj.as_ref().expect("null IDataObject").GetData(&format) {
            Ok(medium) => {
                let hdrop = HDROP(medium.u.hGlobal.0 as _);
                let count = DragQueryFileW(hdrop, u32::MAX, None);
                for i in 0..count {
                    let char_count = DragQueryFileW(hdrop, i, None) as usize;
                    let mut buf = vec![0u16; char_count + 1];
                    DragQueryFileW(hdrop, i, Some(&mut buf));
                    callback(OsString::from_wide(&buf[..char_count]).into());
                }
                Some(hdrop)
            }
            Err(_) => None,
        }
    }

    fn to_position(&self, pt: &POINTL) -> (f64, f64) {
        let mut screen = POINT { x: pt.x, y: pt.y };
        let _ = unsafe { ScreenToClient(self.hwnd, &mut screen) };
        (screen.x as f64, screen.y as f64)
    }
}

#[cfg(target_os = "windows")]
#[allow(non_snake_case)]
impl IDropTarget_Impl for DropTarget_Impl {
    fn DragEnter(
        &self,
        pdataobj: Ref<'_, IDataObject>,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> WinResult<()> {
        let mut paths = Vec::new();
        let hdrop = unsafe { DropTarget::iterate_filenames(pdataobj, |path| paths.push(path)) };
        let valid = hdrop.is_some();
        unsafe {
            *self.enter_is_valid.get() = valid;
        }
        if valid {
            let (x, y) = self.to_position(pt);
            self.forwarder.emit(
                "tauri://drag-enter",
                serde_json::json!({
                    "paths": paths.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
                    "position": { "x": x, "y": y },
                }),
            );
        }
        let effect = if valid { DROPEFFECT_COPY } else { DROPEFFECT_NONE };
        unsafe {
            *pdweffect = effect;
            *self.cursor_effect.get() = effect;
        }
        Ok(())
    }

    fn DragOver(
        &self,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> WinResult<()> {
        if unsafe { *self.enter_is_valid.get() } {
            let (x, y) = self.to_position(pt);
            self.forwarder.emit(
                "tauri://drag-over",
                serde_json::json!({ "position": { "x": x, "y": y } }),
            );
        }
        unsafe { *pdweffect = *self.cursor_effect.get() };
        Ok(())
    }

    fn DragLeave(&self) -> WinResult<()> {
        if unsafe { *self.enter_is_valid.get() } {
            self.forwarder.emit("tauri://drag-leave", serde_json::json!({}));
        }
        Ok(())
    }

    fn Drop(
        &self,
        pdataobj: Ref<'_, IDataObject>,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        _pdweffect: *mut DROPEFFECT,
    ) -> WinResult<()> {
        if unsafe { *self.enter_is_valid.get() } {
            let (x, y) = self.to_position(pt);
            let mut paths = Vec::new();
            let hdrop = unsafe { DropTarget::iterate_filenames(pdataobj, |path| paths.push(path)) };
            self.forwarder.emit(
                "tauri://drag-drop",
                serde_json::json!({
                    "paths": paths.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
                    "position": { "x": x, "y": y },
                }),
            );
            if let Some(hdrop) = hdrop {
                unsafe { DragFinish(hdrop) };
            }
        }
        Ok(())
    }
}

/// 从应用拖出路径（Windows OLE DoDragDrop）。阻塞直到拖拽结束。
#[cfg(target_os = "windows")]
pub fn start_drag_out(paths: Vec<String>) -> Result<(), String> {
    let existing: Vec<PathBuf> = paths
        .into_iter()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect();
    if existing.is_empty() {
        return Err("没有可拖出的路径（路径不存在）".into());
    }

    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if hr.0 == RPC_E_CHANGED_MODE.0 {
            return Err("OLE 初始化失败：当前线程不支持拖拽".into());
        }
        let need_uninit = hr.0 == 0; // S_OK：本次调用完成了初始化
        let result = drag_drop_loop(&existing);
        if need_uninit {
            CoUninitialize();
        }
        result?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
unsafe fn drag_drop_loop(paths: &[PathBuf]) -> Result<(), String> {
    let data_object: IDataObject = DragSourceData::new(paths.to_vec()).into();
    let drop_source: IDropSource = DragSource::new().into();
    let mut effect = DROPEFFECT_NONE;
    let hr = DoDragDrop(
        &data_object,
        &drop_source,
        DROPEFFECT_COPY | DROPEFFECT_MOVE,
        &mut effect,
    );

    if hr.is_err() {
        return Err(format!("Windows 拖拽启动失败：HRESULT 0x{:08X}", hr.0 as u32));
    }
    if hr == DRAGDROP_S_CANCEL {
        return Err("拖拽已取消".into());
    }
    if hr != DRAGDROP_S_DROP && hr != DRAGDROP_S_USEDEFAULTCURSORS {
        return Err(format!("Windows 拖拽未完成：HRESULT 0x{:08X}", hr.0 as u32));
    }
    Ok(())
}

/// 拖出数据源：提供 CF_HDROP 格式的路径列表。
#[cfg(target_os = "windows")]
#[implement(IDataObject)]
struct DragSourceData {
    paths: Vec<PathBuf>,
}

#[cfg(target_os = "windows")]
impl DragSourceData {
    fn new(paths: Vec<PathBuf>) -> Self {
        Self { paths }
    }

    /// 构建 CF_HDROP 全局内存：DROPFILES 头 + UTF-16 路径列表（双 null 结尾）。
    unsafe fn build_hdrop(&self) -> WinResult<HGLOBAL> {
        let header_size = mem::size_of::<DROPFILES>();
        let mut strings: Vec<Vec<u16>> = Vec::with_capacity(self.paths.len());
        let mut total = header_size;
        for path in &self.paths {
            let wide: Vec<u16> = path.to_string_lossy().encode_utf16().collect();
            total += (wide.len() + 1) * mem::size_of::<u16>();
            strings.push(wide);
        }
        total += mem::size_of::<u16>(); // 结尾双 null

        let handle = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, total)?;
        let base = GlobalLock(handle) as *mut u8;
        if base.is_null() {
            let _ = GlobalUnlock(handle);
            return Err(windows::core::Error::from_win32());
        }

        let header = base as *mut DROPFILES;
        (*header).pFiles = header_size as u32;
        (*header).fWide = BOOL(1);

        let mut offset = header_size;
        for wide in &strings {
            for &unit in wide {
                *(base.add(offset) as *mut u16) = unit;
                offset += mem::size_of::<u16>();
            }
            *(base.add(offset) as *mut u16) = 0;
            offset += mem::size_of::<u16>();
        }
        *(base.add(offset) as *mut u16) = 0;

        let _ = GlobalUnlock(handle);
        Ok(handle)
    }
}

#[cfg(target_os = "windows")]
#[allow(non_snake_case)]
impl IDataObject_Impl for DragSourceData_Impl {
    fn GetData(&self, pformatetc: *const FORMATETC) -> WinResult<STGMEDIUM> {
        unsafe {
            let format = &*pformatetc;
            if format.cfFormat != CF_HDROP.0 {
                return Err(E_NOTIMPL.into());
            }
            let handle = self.build_hdrop()?;
            let mut medium = STGMEDIUM::default();
            medium.tymed = TYMED_HGLOBAL.0 as u32;
            medium.u.hGlobal = handle;
            Ok(medium)
        }
    }

    fn GetDataHere(&self, _: *const FORMATETC, _: *mut STGMEDIUM) -> WinResult<()> {
        Err(E_NOTIMPL.into())
    }

    fn QueryGetData(&self, pformatetc: *const FORMATETC) -> HRESULT {
        unsafe {
            if (*pformatetc).cfFormat == CF_HDROP.0 {
                HRESULT(0)
            } else {
                E_NOTIMPL
            }
        }
    }

    fn GetCanonicalFormatEtc(&self, _: *const FORMATETC, pformatetcout: *mut FORMATETC) -> HRESULT {
        unsafe {
            ptr::write(pformatetcout, FORMATETC::default());
        }
        // DATA_S_SAMEFORMATETC：请求与传入相同的格式
        HRESULT(0x0004_0103)
    }

    fn SetData(&self, _: *const FORMATETC, _: *const STGMEDIUM, _: BOOL) -> WinResult<()> {
        Err(E_NOTIMPL.into())
    }

    fn EnumFormatEtc(&self, _: u32) -> WinResult<IEnumFORMATETC> {
        Err(E_NOTIMPL.into())
    }

    fn DAdvise(&self, _: *const FORMATETC, _: u32, _: Ref<'_, IAdviseSink>) -> WinResult<u32> {
        Err(E_NOTIMPL.into())
    }

    fn DUnadvise(&self, _: u32) -> WinResult<()> {
        Err(E_NOTIMPL.into())
    }

    fn EnumDAdvise(&self) -> WinResult<IEnumSTATDATA> {
        Err(E_NOTIMPL.into())
    }
}

/// 拖出交互源：控制拖拽的取消/放下时机与光标。
#[cfg(target_os = "windows")]
#[implement(IDropSource)]
struct DragSource;

#[cfg(target_os = "windows")]
impl DragSource {
    fn new() -> Self {
        Self
    }
}

#[cfg(target_os = "windows")]
#[allow(non_snake_case)]
impl IDropSource_Impl for DragSource_Impl {
    fn QueryContinueDrag(&self, fescape: BOOL, grfkeystate: MODIFIERKEYS_FLAGS) -> HRESULT {
        if fescape.as_bool() {
            return DRAGDROP_S_CANCEL;
        }
        let flags = grfkeystate.0;
        if flags & (MK_LBUTTON | MK_MBUTTON) == 0 {
            // 所有鼠标按键已释放 → 放下
            return DRAGDROP_S_DROP;
        }
        if flags & MK_RBUTTON != 0 {
            // 右键按下 → 取消
            return DRAGDROP_S_CANCEL;
        }
        HRESULT(0) // S_OK 继续拖拽
    }

    fn GiveFeedback(&self, _dweffect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}
