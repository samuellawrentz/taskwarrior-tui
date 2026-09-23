#[derive(Clone, PartialEq, Eq, Debug, Copy)]
pub enum Action {
  Report,
  Search,
  Add,
  Annotate,
  Subprocess,
  Log,
  Modify,
  HelpPopup,
  ContextMenu,
  ReportMenu,
  Picker,
  Jump,
  DeletePrompt,
  UndoPrompt,
  DonePrompt,
  Error,
}
