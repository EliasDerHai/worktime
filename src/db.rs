use chrono::Local;
use sqlx::SqlitePool;

pub async fn start(pool: &SqlitePool) -> sqlx::Result<()> {
    let now = Local::now().naive_local();
    sqlx::query!("INSERT INTO work_sessions (start_time) VALUES ($1)", now)
        .execute(pool)
        .await?;
    println!("Started at {}", now.format("%H:%M"));
    Ok(())
}

pub async fn stop(pool: &SqlitePool) -> sqlx::Result<()> {
    let now = Local::now().naive_local();
    let updated = sqlx::query!(
        r#"
        UPDATE work_sessions
           SET stop_time = $1
         WHERE id = (
             SELECT id
               FROM work_sessions
              WHERE stop_time IS NULL
              ORDER BY start_time DESC
              LIMIT 1
         )
        "#,
        now
    )
    .execute(pool)
    .await?;

    if updated.rows_affected() == 0 {
        println!("No running session found.");
    } else {
        println!("Stopped at {}", now.format("%H:%M"));
    }

    Ok(())
}

pub async fn report(pool: &SqlitePool) -> sqlx::Result<()> {
    let today = Local::now().naive_local().date();

    let rows = sqlx::query!(
        r#"
        SELECT * FROM work_sessions
        WHERE date(start_time) = date($1)
        "#,
        today
    )
    .fetch_all(pool)
    .await?;

    let mut total_seconds = 0;

    for row in rows {
        let start = row.start_time;

        if let Some(stop) = row.stop_time {
            println!("start: {start} - stop: {stop:?}");
        } else {
            println!("Clock still running - use `cargo run -- stop` to stop");
            return Ok(());
        }
    }

    let hours = total_seconds as f64 / 3600.0;
    println!("Total work time for today: {:.2} hours", hours);

    Ok(())
}
