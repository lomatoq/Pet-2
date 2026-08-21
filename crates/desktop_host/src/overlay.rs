#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayPolicy {
    pub always_on_top: bool,
    pub hidden_from_task_switcher: bool,
    pub accepts_input: bool,
}

impl Default for OverlayPolicy {
    fn default() -> Self {
        Self {
            always_on_top: true,
            hidden_from_task_switcher: true,
            accepts_input: false,
        }
    }
}
