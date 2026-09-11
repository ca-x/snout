use crate::i18n::{L10n, Lang};
use anyhow::{ensure, Context, Result};
use fcitx5_dbus::controller::ControllerProxy;
use std::time::Duration;
use zbus::{fdo::DBusProxy, Connection};

const SERVICE: &str = "org.fcitx.Fcitx5";

/// Add Rime to the current group without changing the user's input method selection.
/// Returns false when Rime was already in the group.
pub async fn ensure_rime(lang: Lang) -> Result<bool> {
    let t = L10n::new(lang);
    tokio::time::timeout(Duration::from_secs(5), async {
        let connection = Connection::session()
            .await
            .context(t.t("fcitx5.setup.session_unavailable").to_string())?;
        ensure_rime_with_connection(&connection, &t).await
    })
    .await
    .context(t.t("fcitx5.setup.timeout").to_string())?
}

async fn ensure_rime_with_connection(connection: &Connection, t: &L10n) -> Result<bool> {
    // Do not activate another input framework in an IBus or headless session.
    let bus = DBusProxy::new(connection).await?;
    ensure!(
        bus.name_has_owner(SERVICE.try_into()?).await?,
        "{}",
        t.t("fcitx5.setup.not_running")
    );
    let proxy = ControllerProxy::new(connection).await?;

    let group = proxy.current_input_method_group().await?;
    ensure!(!group.is_empty(), "{}", t.t("fcitx5.setup.no_group"));
    let (layout, mut entries) = proxy.input_method_group_info(&group).await?;
    if entries.iter().any(|(name, _)| name == "rime") {
        return Ok(false);
    }

    ensure!(
        proxy
            .available_input_methods()
            .await?
            .iter()
            .any(|entry| entry.0 == "rime"),
        "{}",
        t.t("fcitx5.setup.rime_missing")
    );

    entries.push(("rime".into(), String::new()));
    let entries: Vec<_> = entries
        .iter()
        .map(|(name, layout)| (name.as_str(), layout.as_str()))
        .collect();
    // Fcitx5 saves the group itself; editing its profile on disk can be overwritten
    // by the running daemon. Keep the existing layout and entry order intact.
    if let Err(error) = proxy
        .set_input_method_group_info(&group, &layout, &entries)
        .await
    {
        let key = if matches!(
            &error,
            zbus::Error::MethodError(name, ..)
                if name.as_str() == "org.freedesktop.DBus.Error.AccessDenied"
        ) {
            "fcitx5.setup.permission_denied"
        } else {
            "fcitx5.setup.save_failed"
        };
        return Err(error).context(t.t(key).to_string());
    }

    // The controller can silently ignore a group removed by another client.
    let (_, saved_entries) = proxy.input_method_group_info(&group).await?;
    ensure!(
        saved_entries.iter().any(|(name, _)| name == "rime"),
        "{}",
        t.t("fcitx5.setup.save_failed")
    );
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::{BufRead, BufReader};
    use std::process::{Child, Command, Stdio};
    use std::sync::{Arc, Mutex};

    type Group = (String, Vec<(String, String)>);
    type AvailableMethod = (String, String, String, String, String, String, bool);

    // A private session bus exercises the real D-Bus signatures without touching
    // the developer's desktop or changing process-wide environment variables.
    struct TestBus {
        child: Child,
        address: String,
    }

    impl TestBus {
        fn new() -> Self {
            let child = Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--nopidfile", "--print-address=1"])
                .stdout(Stdio::piped())
                .spawn()
                .expect("Linux D-Bus tests require dbus-daemon");
            let mut bus = Self {
                child,
                address: String::new(),
            };
            BufReader::new(bus.child.stdout.take().expect("bus stdout"))
                .read_line(&mut bus.address)
                .expect("read bus address");
            bus.address = bus.address.trim().to_string();
            bus
        }

        async fn connect(&self) -> Connection {
            zbus::connection::Builder::address(self.address.as_str())
                .unwrap()
                .build()
                .await
                .unwrap()
        }
    }

    impl Drop for TestBus {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    struct State {
        current: String,
        groups: HashMap<String, Group>,
        rime_available: bool,
        available_queries: usize,
        available_delay: Duration,
        reject_write: bool,
        ignore_write: bool,
        writes: usize,
    }

    impl Default for State {
        fn default() -> Self {
            Self {
                current: "工作".into(),
                groups: HashMap::from([
                    (
                        "工作".into(),
                        (
                            "de".into(),
                            vec![
                                ("keyboard-de".into(), String::new()),
                                ("pinyin".into(), "us".into()),
                            ],
                        ),
                    ),
                    (
                        "Other".into(),
                        ("us".into(), vec![("keyboard-us".into(), String::new())]),
                    ),
                ]),
                rime_available: true,
                available_queries: 0,
                available_delay: Duration::ZERO,
                reject_write: false,
                ignore_write: false,
                writes: 0,
            }
        }
    }

    struct Controller(Arc<Mutex<State>>);

    #[zbus::interface(name = "org.fcitx.Fcitx.Controller1")]
    impl Controller {
        async fn available_input_methods(&self) -> Vec<AvailableMethod> {
            let delay = {
                let mut state = self.0.lock().unwrap();
                state.available_queries += 1;
                state.available_delay
            };
            tokio::time::sleep(delay).await;
            if self.0.lock().unwrap().rime_available {
                vec![(
                    "rime".into(),
                    "Rime".into(),
                    String::new(),
                    String::new(),
                    String::new(),
                    "zh".into(),
                    true,
                )]
            } else {
                Vec::new()
            }
        }

        fn current_input_method_group(&self) -> String {
            self.0.lock().unwrap().current.clone()
        }

        fn input_method_group_info(&self, group: &str) -> Group {
            self.0
                .lock()
                .unwrap()
                .groups
                .get(group)
                .cloned()
                .unwrap_or_default()
        }

        fn set_input_method_group_info(
            &mut self,
            group: &str,
            layout: &str,
            entries: Vec<(String, String)>,
        ) -> zbus::fdo::Result<()> {
            let mut state = self.0.lock().unwrap();
            state.writes += 1;
            if state.reject_write {
                return Err(zbus::fdo::Error::AccessDenied("read-only profile".into()));
            }
            if !state.ignore_write && state.groups.contains_key(group) {
                state.groups.insert(group.into(), (layout.into(), entries));
            }
            Ok(())
        }
    }

    async fn serve(bus: &TestBus, state: Arc<Mutex<State>>) -> Connection {
        zbus::connection::Builder::address(bus.address.as_str())
            .unwrap()
            .name(SERVICE)
            .unwrap()
            .serve_at("/controller", Controller(state))
            .unwrap()
            .build()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn adds_rime_preserving_groups_layouts_and_order_and_is_idempotent() {
        let bus = TestBus::new();
        let state = Arc::new(Mutex::new(State::default()));
        let original_groups = state.lock().unwrap().groups.clone();
        let _server = serve(&bus, state.clone()).await;
        let client = bus.connect().await;
        let t = L10n::new(Lang::En);

        assert!(ensure_rime_with_connection(&client, &t).await.unwrap());
        assert!(!ensure_rime_with_connection(&client, &t).await.unwrap());
        let state = state.lock().unwrap();
        let mut expected = original_groups;
        expected
            .get_mut("工作")
            .unwrap()
            .1
            .push(("rime".into(), String::new()));
        assert_eq!(state.groups, expected);
        assert_eq!(state.current, "工作");
        assert_eq!(state.writes, 1);
    }

    #[tokio::test]
    async fn existing_rime_keeps_its_position_and_layout_without_writing() {
        let bus = TestBus::new();
        let mut initial = State {
            available_delay: Duration::from_millis(120),
            ..State::default()
        };
        initial
            .groups
            .get_mut("工作")
            .unwrap()
            .1
            .insert(1, ("rime".into(), "fr".into()));
        let original = initial.groups.clone();
        let state = Arc::new(Mutex::new(initial));
        let _server = serve(&bus, state.clone()).await;
        let started = std::time::Instant::now();
        assert!(
            !ensure_rime_with_connection(&bus.connect().await, &L10n::new(Lang::Zh))
                .await
                .unwrap()
        );
        let state = state.lock().unwrap();
        eprintln!("Existing Rime check: {:?}", started.elapsed());
        assert_eq!(state.groups, original);
        assert_eq!(state.writes, 0);
        assert_eq!(
            state.available_queries, 0,
            "existing Rime must skip enumeration"
        );
    }

    #[tokio::test]
    async fn missing_service_reports_how_to_start_fcitx5() {
        let bus = TestBus::new();
        let t = L10n::new(Lang::En);
        let error = ensure_rime_with_connection(&bus.connect().await, &t)
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), t.t("fcitx5.setup.not_running"));
    }

    #[tokio::test]
    async fn missing_rime_or_group_leaves_configuration_untouched() {
        for (rime_available, current, key) in [
            (false, "工作", "fcitx5.setup.rime_missing"),
            (true, "", "fcitx5.setup.no_group"),
        ] {
            let bus = TestBus::new();
            let initial = State {
                rime_available,
                current: current.into(),
                ..State::default()
            };
            let original = initial.groups.clone();
            let state = Arc::new(Mutex::new(initial));
            let _server = serve(&bus, state.clone()).await;
            for lang in [Lang::Zh, Lang::En] {
                let t = L10n::new(lang);
                let error = ensure_rime_with_connection(&bus.connect().await, &t)
                    .await
                    .unwrap_err();
                assert_ne!(t.t(key), key, "missing translation");
                assert_eq!(error.to_string(), t.t(key));
            }
            let state = state.lock().unwrap();
            assert_eq!(state.groups, original);
            assert_eq!(state.writes, 0);
        }
    }

    #[tokio::test]
    async fn rejected_or_silently_ignored_writes_are_reported_as_failures() {
        for reject_write in [true, false] {
            let bus = TestBus::new();
            let initial = State {
                reject_write,
                ignore_write: !reject_write,
                ..State::default()
            };
            let original = initial.groups.clone();
            let state = Arc::new(Mutex::new(initial));
            let _server = serve(&bus, state.clone()).await;
            for lang in [Lang::Zh, Lang::En] {
                let t = L10n::new(lang);
                let error = ensure_rime_with_connection(&bus.connect().await, &t)
                    .await
                    .unwrap_err();
                let key = if reject_write {
                    "fcitx5.setup.permission_denied"
                } else {
                    "fcitx5.setup.save_failed"
                };
                assert_ne!(t.t(key), key, "missing translation");
                assert_eq!(error.to_string(), t.t(key));
                if reject_write {
                    assert!(format!("{error:#}").contains("read-only profile"));
                }
            }
            let state = state.lock().unwrap();
            assert_eq!(state.groups, original);
            assert_eq!(state.writes, 2);
        }
    }
}
