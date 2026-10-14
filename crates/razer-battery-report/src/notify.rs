use notify_rust::Notification;

pub struct Notify {
    app_name: String,
}

impl Notify {
    pub fn new() -> Self {
        #[cfg(target_os = "windows")]
        Self {
            app_name: String::from("Razer Battery Report"),
        }
    }

    pub fn battery_low(
        &self,
        device_name: &str,
        battery_level: i32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Notification::new()
            .summary(&self.app_name)
            .body(&format!(
                "{}: Battery low ({}%)",
                device_name, battery_level
            ))
            .show()?;
        Ok(())
    }

    pub fn battery_full(&self, device_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        Notification::new()
            .summary(&self.app_name)
            .body(&format!("{}: Battery fully charged", device_name))
            .show()?;
        Ok(())
    }

    pub fn device_connected(&self, device_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        Notification::new()
            .summary(&self.app_name)
            .body(&format!("{}: Connected", device_name))
            .show()?;
        Ok(())
    }

    pub fn device_disconnected(&self, device_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        Notification::new()
            .summary(&self.app_name)
            .body(&format!("{}: Disconnected", device_name))
            .show()?;
        Ok(())
    }
}
