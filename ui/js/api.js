const T = window.__TAURI__;
export const invoke = (cmd, args) => T.core.invoke(cmd, args);
export const listen = (ev, fn) => T.event.listen(ev, (e) => fn(e.payload));
export const fileUrl = (p) => T.core.convertFileSrc(p);
export const pickVideo = () =>
  T.dialog.open({ multiple: false, filters: [{ name: 'Video', extensions: ['mp4', 'mov', 'm4v'] }] });
export const pickDir = (def) => T.dialog.open({ directory: true, defaultPath: def });
export const onDrop = (fn) => listen('tauri://drag-drop', (p) => fn(p.paths || []));
export const onDragState = (over, leave) => {
  listen('tauri://drag-enter', over);
  listen('tauri://drag-over', over);
  listen('tauri://drag-leave', leave);
  listen('tauri://drag-drop', leave);
};
