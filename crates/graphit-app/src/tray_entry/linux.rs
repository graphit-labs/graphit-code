use super::{Action, LoginProfile};
use ksni::{
    MenuItem,
    menu::{StandardItem, SubMenu},
};
use tokio::sync::mpsc::UnboundedSender;

pub(super) struct Tray {
    pub name: String,
    pub state: String,
    pub error: Option<String>,
    pub profiles: Vec<LoginProfile>,
    pub busy: bool,
    pub running: bool,
    pub quitting: bool,
    pub sender: UnboundedSender<Action>,
}

impl Tray {
    fn send(&mut self, action: Action) {
        if matches!(action, Action::Quit) {
            if self.quitting {
                return;
            }
            self.quitting = true;
        } else {
            if self.busy || self.quitting {
                return;
            }
            if !matches!(action, Action::Refresh) {
                self.busy = true;
            }
        }
        let _ = self.sender.send(action);
    }

    fn item(&self, label: &str, action: Action) -> MenuItem<Self> {
        StandardItem {
            label: label.into(),
            enabled: !self.busy && !self.quitting,
            activate: Box::new(move |tray: &mut Tray| tray.send(action.clone())),
            ..Default::default()
        }
        .into()
    }
}

impl ksni::Tray for Tray {
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        self.name.clone()
    }
    fn title(&self) -> String {
        self.name.clone()
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![ksni::Icon {
            width: 32,
            height: 32,
            data: include_bytes!("icon.argb").to_vec(),
        }]
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: self.name.clone(),
            description: self.error.as_ref().unwrap_or(&self.state).clone(),
            ..Default::default()
        }
    }
    fn status(&self) -> ksni::Status {
        if self.error.is_some() {
            ksni::Status::NeedsAttention
        } else {
            ksni::Status::Active
        }
    }
    fn menu_about_to_show(&mut self) {
        self.send(Action::Refresh);
    }
    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items = vec![
            StandardItem {
                label: format!("{}: {}", self.name, self.state),
                enabled: false,
                ..Default::default()
            }
            .into(),
        ];
        if let Some(error) = &self.error {
            items.push(
                StandardItem {
                    label: error.clone(),
                    enabled: false,
                    ..Default::default()
                }
                .into(),
            );
        }
        items.push(MenuItem::Separator);
        items.push(self.item("Status", Action::Refresh));
        items.push(self.item("Start daemon", Action::Start));
        items.push(self.item("Restart daemon", Action::Restart));
        items.push(self.item("Open interface", Action::OpenUi));
        let mut providers = std::collections::BTreeMap::<&str, Vec<MenuItem<Self>>>::new();
        for profile in &self.profiles {
            providers
                .entry(&profile.provider)
                .or_default()
                .push(self.item(
                    &profile.name.replace('_', "__"),
                    Action::Login(profile.clone()),
                ));
        }
        let login = if providers.is_empty() {
            vec![
                StandardItem {
                    label: if self.running {
                        format!("Configure a profile with {} login --help", self.name)
                    } else {
                        "Start daemon to list configured profiles".into()
                    },
                    enabled: false,
                    ..Default::default()
                }
                .into(),
            ]
        } else {
            providers
                .into_iter()
                .map(|(provider, submenu)| {
                    SubMenu {
                        label: provider.replace('_', "__"),
                        enabled: !self.busy && !self.quitting,
                        submenu,
                        ..Default::default()
                    }
                    .into()
                })
                .collect()
        };
        items.push(
            SubMenu {
                label: "Log in".into(),
                submenu: login,
                ..Default::default()
            }
            .into(),
        );
        items.push(MenuItem::Separator);
        items.push(
            StandardItem {
                label: "Quit and stop daemon".into(),
                enabled: !self.quitting,
                activate: Box::new(|tray: &mut Tray| tray.send(Action::Quit)),
                ..Default::default()
            }
            .into(),
        );
        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ksni::Tray as _;

    #[test]
    fn opening_menu_keeps_the_interface_action_available_during_refresh() {
        let (sender, mut actions) = tokio::sync::mpsc::unbounded_channel();
        let mut tray = Tray {
            name: "graphit".into(),
            state: "Daemon running".into(),
            error: None,
            profiles: Vec::new(),
            busy: false,
            quitting: false,
            running: true,
            sender,
        };
        tray.menu_about_to_show();
        let item = tray
            .menu()
            .into_iter()
            .find_map(|item| match item {
                MenuItem::Standard(item) if item.label == "Open interface" => Some(item),
                _ => None,
            })
            .unwrap();
        assert!(item.enabled, "status refresh must preserve menu actions");
        (item.activate)(&mut tray);
        assert!(matches!(actions.try_recv().unwrap(), Action::Refresh));
        assert!(matches!(actions.try_recv().unwrap(), Action::OpenUi));
        // The explicit action retains the existing guard against duplicate work.
        tray.menu_about_to_show();
        assert!(actions.try_recv().is_err());
    }
}
