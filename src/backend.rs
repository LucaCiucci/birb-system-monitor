use tokio::runtime::Runtime;

pub mod sysinfo;

pub struct Systems {
    rt: Runtime,
}

impl Systems {
    pub fn new() -> Self {
        Self {
            rt: Runtime::new().unwrap(),
        }
    }
}
