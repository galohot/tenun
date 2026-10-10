//! The command-line publisher: `tenun login`, `tenun publish`, `tenun agent`.
//! The agent is the same publisher as the app, for a computer with no screen:
//! it keeps a workspace's published dashboards updated on their schedules.

use std::path::Path;
use std::time::Duration;
use tenun::cloud;
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
    Ok(engine)
}

fn find(engine: &Engine, want: &str) -> Option<String> {
    engine.dashboards().into_iter().find_map(|d| {
        let id = d["id"].as_str()?;
        (id == want || d["title"].as_str() == Some(want)).then(|| id.to_string())
    })
}

pub fn publish_now(file: &Path, dashboard: Option<&str>) -> anyhow::Result<()> {
    let engine = open(file)?;
    let base = cloud::base_url();
    let ids: Vec<String> = match dashboard {
        Some(want) => vec![
            find(&engine, want)
                .ok_or_else(|| anyhow::anyhow!("no dashboard {want} in {}", file.display()))?,
        ],
        None => engine
            .dashboards()
            .iter()
            .filter(|d| cloud::settings(d).is_some())
            .filter_map(|d| d["id"].as_str().map(str::to_string))
            .collect(),
    };
    if ids.is_empty() {
        anyhow::bail!(
            "no dashboard is set to publish; publish one from the app first, or name it with --dashboard"
        );
    }
    let mut failed = false;
    for id in ids {
        match cloud::publish(&engine, &secrets(), &base, &id) {
            Ok(v) => println!("published {id}: {}", v["url"].as_str().unwrap_or_default()),
            Err(e) => {
                failed = true;
                eprintln!("{id}: {e}");
            }
        }
    }
    if failed {
        anyhow::bail!("some dashboards were not published");
    }
    Ok(())
}

pub fn run(file: &Path) -> anyhow::Result<()> {
    let base = cloud::base_url();
    let s = secrets();
    if cloud::connection(&s, &base).is_none() {
        anyhow::bail!("this computer is not connected; run `tenun login` first");
    }
    let engine = std::sync::Arc::new(open(file)?);
    let schedule = std::sync::Arc::new(std::sync::Mutex::new(cloud::Schedule::default()));

    // Data files that change mark "on change" dashboards as due.
    let (e2, s2) = (engine.clone(), schedule.clone());
    let mut watcher = tenun_core::watch::FileWatcher::new(move |paths| {
        for (id, p) in e2.watched_files() {
            if paths.contains(&tenun_core::watch::canonical(&p)) {
                e2.load(&id);
                log(&format!("{} changed", p.display()));
            }
        }
        s2.lock().unwrap().mark_changed(&e2);
    })?;
    let files: Vec<_> = engine.watched_files().into_iter().map(|(_, p)| p).collect();
    watcher.set(&files);

    let scheduled: Vec<String> = engine
        .dashboards()
        .iter()
        .filter_map(|d| {
            let ps = cloud::settings(d)?;
            (ps.every != cloud::Every::Manual).then(|| {
                format!("{} ({:?})", d["title"].as_str().unwrap_or("?"), ps.every).to_lowercase()
            })
        })
        .collect();
    log(&format!(
        "tenun agent {} watching {} — {} scheduled: {}",
        tenun::VERSION,
        file.display(),
        scheduled.len(),
        scheduled.join(", ")
    ));
    loop {
        let (ok, failed) = cloud::run_due(&engine, &s, &base, &mut schedule.lock().unwrap());
        for id in ok {
            log(&format!("published {id}"));
        }
        for (id, e) in failed {
            log(&format!("{id}: {e}"));
        }
        std::thread::sleep(Duration::from_secs(60));
    }
}
