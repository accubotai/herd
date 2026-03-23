use notify_rust::Notification;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum NotifyError {
    #[error("notification failed: {0}")]
    Send(#[from] notify_rust::error::Error),
}

/// Send a desktop notification for a process crash
pub fn notify_crash(process_name: &str, exit_code: Option<i32>) -> Result<(), NotifyError> {
    let body = match exit_code {
        Some(code) => format!("Process '{process_name}' exited with code {code}"),
        None => format!("Process '{process_name}' was killed by a signal"),
    };

    Notification::new()
        .summary("Herd: Process Crashed")
        .body(&body)
        .icon("dialog-warning")
        .urgency(notify_rust::Urgency::Critical)
        .show()?;

    Ok(())
}

/// Send a desktop notification for a process restart
pub fn notify_restart(process_name: &str) -> Result<(), NotifyError> {
    Notification::new()
        .summary("Herd: Process Restarted")
        .body(&format!("Process '{process_name}' has been restarted"))
        .icon("dialog-information")
        .urgency(notify_rust::Urgency::Normal)
        .show()?;

    Ok(())
}

/// Send a generic info notification
pub fn notify_info(title: &str, body: &str) -> Result<(), NotifyError> {
    Notification::new()
        .summary(title)
        .body(body)
        .icon("dialog-information")
        .urgency(notify_rust::Urgency::Normal)
        .show()?;

    Ok(())
}
