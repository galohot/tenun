//! The command-line publisher: `tenun login`, `tenun publish`, `tenun agent`.
//! The agent is the same publisher as the app, for a computer with no screen:
//! it keeps a workspace's published dashboards updated on their schedules.

use std::path::Path;
use std::time::Duration;
use tenun::{alerts, cloud};
use tenun_core::Engine;
use tenun_core::secrets::Secrets;

fn secrets() -> Secrets {
    Secrets::new(Secrets::default_path())
}

fn log(msg: &str) {
    eprintln!("{} {msg}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
}

pub fn login() -> anyhow::Result<()> {
    let base = cloud::base_url();
    let client = cloud::Client::new(base.clone());
    let start = client.start_device(&format!("tenun agent on {}", cloud::source_label()))?;
    println!(
        "To connect this computer to {base}, open:\n\n    {}\n",
        start.verification_uri_complete
    );
    println!("and approve the code {}. Waiting…", start.user_code);
    let deadline = std::time::Instant::now() + Duration::from_secs(start.expires_in);
    while std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_secs(start.interval.max(2)));
        match client.poll_device(&start.device_code)? {
            cloud::Poll::Pending => continue,
            cloud::Poll::Done { token, email } => {
                cloud::save_connection(&secrets(), &base, &token, &email)?;
                println!("Connected as {email}.");
                return Ok(());
            }
            cloud::Poll::Expired => break,
        }
    }
    anyhow::bail!("the code expired before it was approved; run `tenun login` again")
}

pub fn logout() -> anyhow::Result<()> {
    cloud::forget_connection(&secrets(), &cloud::base_url())?;
    println!(
        "This computer no longer publishes to tenun.cloud. (Disconnect it on your account page too.)"
    );
    Ok(())
}

fn open(file: &Path) -> anyhow::Result<Engine> {
    let engine = Engine::new(secrets())?;
    engine.open(file)?;
    let snap = engine.snapshot();
    for (id, e) in &snap.errors {
        log(&format!("warning: source {id} did not load: {e}"));
    }
    // The words of a published page: the table and the column, never the formula (V4-D77).
    for problem in engine.public_table_problems() {
        log(&format!("warning: {problem}"));
    }
    Ok(engine)
}

fn find(engine: &Engine, want: &str) -> Option<String> {
    engine.dashboards().into_iter().find_map(|d| {
        let id = d["id"].as_str()?;
        (id == want || d["title"].as_str() == Some(want)).then(|| id.to_string())
    })
}

pub fn publish_now(
    file: &Path,
    dashboard: Option<&str>,
    password_env: Option<&str>,
) -> anyhow::Result<()> {
    // Read before anything is opened: a missing variable stops here, with nothing changed.
    let password = password_env
        .map(|name| {
            std::env::var(name)
                .map_err(|_| anyhow::anyhow!("the environment variable {name} is not set"))
        })
        .transpose()?;
    let engine = open(file)?;
    let base = cloud::base_url();
    let ids: Vec<String> = match dashboard {
        Some(want) => vec![
            find(&engine, want)
                .ok_or_else(|| anyhow::anyhow!("no dashboard {want} in {}", file.display()))?,
        ],
        // Every dashboard set to publish, and the site when it is (build 14).
        None => engine
            .dashboards()
            .iter()
            .filter(|d| cloud::settings(d).is_some())
            .filter_map(|d| d["id"].as_str().map(str::to_string))
            .chain(cloud::site_publishes(&engine).map(|_| cloud::SITE_ID.to_string()))
            .collect(),
    };
    if ids.is_empty() {
        anyhow::bail!(
            "no dashboard is set to publish; publish one from the app first, or name it with --dashboard"
        );
    }
    if let Some(pw) = &password {
        for id in &ids {
            cloud::set_item_password(&engine, &secrets(), id, pw)?;
        }
    }
    let mut failed = false;
    for id in ids {
        let name = cloud::item_name(&id);
        match cloud::publish_item(&engine, &secrets(), &base, &id) {
            Ok(v) => println!(
                "published {name}: {}",
                v["url"].as_str().unwrap_or_default()
            ),
            Err(e) => {
                failed = true;
                eprintln!("{name}: {e}");
            }
        }
    }
    if failed {
        anyhow::bail!("some dashboards were not published");
    }
    Ok(())
}

/// Checks the alerts once and writes what went (v5 build 12). True when nothing failed.
fn alerts_now(engine: &Engine, s: &Secrets) -> bool {
    let out = alerts::check(engine, s, chrono::Utc::now());
    for (id, text) in &out.sent {
        log(&format!(
            "alert {id} sent: {}",
            text.lines().next().unwrap_or_default()
        ));
    }
    for (id, e) in &out.failed {
        log(&format!("alert {id}: {e}"));
    }
    out.failed.is_empty()
}

/// `tenun alerts check`: every alert that is on, once (v5 build 12). For a scheduled task.
pub fn alerts_once(file: &Path) -> anyhow::Result<()> {
    let engine = open(file)?;
    let on: Vec<_> = engine.alerts().into_iter().filter(|a| a.on).collect();
    if on.is_empty() {
        println!("no alert is on in {}", file.display());
        return Ok(());
    }
    let s = secrets();
    let ok = alerts_now(&engine, &s);
    // What stops an alert that sent nothing: its tile, or no address on this computer.
    let states = alerts::load_states(&alerts::state_path(&s));
    for a in &on {
        if let Some(st) = states.get(&a.id) {
            for e in [&st.problem, &st.error]
                .into_iter()
                .filter(|e| !e.is_empty())
            {
                log(&format!("alert {}: {e}", a.id));
            }
        }
    }
    if !ok {
        anyhow::bail!("some alerts were not sent");
    }
    Ok(())
}

pub fn run(file: &Path) -> anyhow::Result<()> {
    let base = cloud::base_url();
    let s = secrets();
    let connected = cloud::connection(&s, &base).is_some();
    let engine = std::sync::Arc::new(open(file)?);
    // Alerts run without tenun.cloud (v5 build 12).
    if !connected && engine.alerts().iter().all(|a| !a.on) {
        anyhow::bail!(
            "this computer is not connected and the workspace has no alert that is on; run `tenun login` first"
        );
    }
    let schedule = std::sync::Arc::new(std::sync::Mutex::new(cloud::Schedule::default()));

    // Data files that change mark "on change" dashboards as due.
    let (e2, s2) = (engine.clone(), schedule.clone());
    let mut watcher = tenun_core::watch::FileWatcher::new(move |paths| {
        for id in e2.sources_for(&paths) {
            e2.load(&id);
            log(&format!("source {id} changed"));
        }
        s2.lock().unwrap().mark_changed(&e2);
    })?;
    let files: Vec<_> = engine.watched_files().into_iter().map(|(_, p)| p).collect();
    let folders: Vec<_> = engine
        .watched_folders()
        .into_iter()
        .map(|(_, p)| p)
        .collect();
    watcher.set(&files, &folders);

    let scheduled: Vec<String> = engine
        .dashboards()
        .iter()
        .filter_map(|d| {
            let ps = cloud::settings(d)?;
            (ps.every != cloud::Every::Manual).then(|| {
                format!("{} ({:?})", d["title"].as_str().unwrap_or("?"), ps.every).to_lowercase()
            })
        })
        .chain(
            cloud::site_publishes(&engine)
                .filter(|ps| ps.every != cloud::Every::Manual)
                .map(|ps| format!("the site ({:?})", ps.every).to_lowercase()),
        )
        .collect();
    log(&format!(
        "tenun agent {} watching {} — {} scheduled: {}; {} alerts on",
        tenun::VERSION,
        file.display(),
        scheduled.len(),
        scheduled.join(", "),
        engine.alerts().iter().filter(|a| a.on).count()
    ));
    loop {
        // CSV links are fetched again on their own schedule, and dashboards that publish
        // "on change" are due when one of them gave new data.
        // A calculated column with `TODAY()` is built again when the day has changed (V4-D34).
        if engine.refresh_dates() {
            log("a new day: calculated columns built again");
            schedule.lock().unwrap().mark_changed(&engine);
        }
        let urls = engine.refresh_due_urls();
        if !urls.is_empty() {
            log(&format!("fetched {}", urls.join(", ")));
            schedule.lock().unwrap().mark_changed(&engine);
        }
        alerts_now(&engine, &s);
        if connected {
            let (ok, failed) = cloud::run_due(&engine, &s, &base, &mut schedule.lock().unwrap());
            for id in ok {
                log(&format!("published {}", cloud::item_name(&id)));
            }
            for (id, e) in failed {
                log(&format!("{}: {e}", cloud::item_name(&id)));
            }
        }
        std::thread::sleep(Duration::from_secs(60));
    }
}
