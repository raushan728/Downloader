use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

lazy_static! {
    pub static ref DOWNLOAD_TASKS: Arc<Mutex<HashMap<String, String>>> =
        Arc::new(Mutex::new(HashMap::new()));
}

pub fn update_task(task_id: &str, status: &str) {
    if let Ok(mut tasks) = DOWNLOAD_TASKS.lock() {
        tasks.insert(task_id.to_string(), status.to_string());
    }
}

pub fn get_task(task_id: &str) -> Option<String> {
    if let Ok(tasks) = DOWNLOAD_TASKS.lock() {
        tasks.get(task_id).cloned()
    } else {
        None
    }
}
