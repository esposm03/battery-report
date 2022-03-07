use battery::{units::ratio::ratio, Battery, Error, Manager};
use once_cell::sync::OnceCell;
use time::OffsetDateTime;
use tray_item::TrayItem;

use std::{
    fs::File,
    io::{BufWriter, Write},
    process,
    sync::Mutex,
    thread,
    time::Duration,
};

static MEASUREMENTS: OnceCell<Mutex<Vec<(OffsetDateTime, f32)>>> = OnceCell::new();
static HTML: &'static str = include_str!("./index.html");
static PLOTLY: &'static str = include_str!("./plotly-2.9.0.min.js");

fn main() -> Result<(), Error> {
    let battery = get_battery();
    let percentages = vec![(
        OffsetDateTime::now_utc(),
        battery.state_of_charge().get::<ratio>(),
    )];
    MEASUREMENTS.set(Mutex::new(percentages)).unwrap();

    ctrlc::set_handler(move || {
        let mut battery = get_battery();

        update(&mut battery).unwrap();

        let data = MEASUREMENTS.get().unwrap().lock().unwrap();
        let _elapsed = data.last().unwrap().0 - data.first().unwrap().0;

        process::exit(0);
    })
    .unwrap();

    thread::spawn(|| {
        let mut battery = get_battery();
        loop {
            thread::sleep(Duration::from_secs(300));
            update(&mut battery).expect("Failed to update battery status");
        }
    });

    setup_tray();

    Ok(())
}

/// Update `DATA`
fn update(battery: &mut Battery) -> Result<(), Error> {
    battery.refresh()?;

    let time = OffsetDateTime::now_utc();
    let percentage = battery.state_of_charge().get::<ratio>();

    let mut lock = MEASUREMENTS.get().unwrap().lock().unwrap();
    lock.push((time, percentage));

    Ok(())
}

/// Get the first battery, or panic on failure
fn get_battery() -> Battery {
    Manager::new()
        .unwrap()
        .batteries()
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
}

/// Function that sets up the tray, and blocks forever
fn setup_tray() {
    let mut tray = TrayItem::new("Battery Status", "").unwrap();
    tray.add_menu_item("Show data", || {
        let mut battery = get_battery();

        update(&mut battery).unwrap();

        render_template().unwrap();
        open::that("/tmp/battery_report.html").unwrap();
    })
    .unwrap();
    tray.add_menu_item("Quit", || {
        let mut battery = get_battery();

        update(&mut battery).unwrap();

        render_template().unwrap();
        open::that("/tmp/battery_report.html").unwrap();

        process::exit(0);
    })
    .unwrap();
    tray.inner_mut().display();
}

fn render_template() -> Result<(), std::io::Error> {
    let lock = MEASUREMENTS.get().unwrap().lock().unwrap();
    let (x, y): (Vec<_>, Vec<_>) = lock.iter().cloned().unzip();
    let elapsed = lock.last().unwrap().0 - lock.first().unwrap().0;
    let battery_change = lock.last().unwrap().1 - lock.first().unwrap().1;

    let string = HTML
        .to_string()
        .replace("{plotly}", PLOTLY)
        .replace(
            "{x}",
            &x.iter()
                .map(|date| {
                    format!(
                        "'{:04}-{:02}-{:02} {:02}:{:02}:{:02}'",
                        date.year(),
                        date.month() as u8,
                        date.day(),
                        date.hour(),
                        date.minute(),
                        date.second(),
                    )
                })
                .collect::<Vec<_>>()
                .join(","),
        )
        .replace(
            "{y}",
            &y.iter()
                .map(|perc| format!("'{:.2}%'", perc * 100.0))
                .collect::<Vec<_>>()
                .join(","),
        )
        .replace(
            "{title}",
            &format!(
                "{:.2}% in {}h {}m",
                battery_change,
                elapsed.whole_hours(),
                elapsed.whole_minutes() % 60,
            ),
        );

    let mut file = BufWriter::new(File::create("/tmp/battery_report.html")?);
    file.write_all(string.as_bytes())?;

    Ok(())
}
