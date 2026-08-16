interface ConfirmDialogProps {
  title: string;
  message: string;
  onSave: () => void;
  onDiscard: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({ title, message, onSave, onDiscard, onCancel }: ConfirmDialogProps) {
  return (
    <div className="modal-mask" onClick={onCancel}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3 style={{ margin: "0 0 8px", fontSize: 14 }}>{title}</h3>
        <p style={{ margin: "0 0 16px", fontSize: 13, color: "#666" }}>{message}</p>
        <div className="modal-actions">
          <button className="btn" onClick={onCancel}>
            取消
          </button>
          <button className="btn" onClick={onDiscard}>
            放弃
          </button>
          <button className="btn btn-primary" onClick={onSave}>
            保存
          </button>
        </div>
      </div>
    </div>
  );
}
