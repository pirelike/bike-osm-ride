#[cfg(test)]
mod tests {
    /// The point of this module: without the init call the hub panics instead of reporting.
    #[test]
    fn the_backend_is_initialized_before_the_hub_starts() {
        const HUB: &str = include_str!("lib.rs");
        assert!(
            HUB.contains("init_backend()"),
            "spawn_sensor_hub must initialize the Android backend"
        );
        assert!(HUB.contains("android_ble"), "the hub uses this module");
    }

    /// Without the activity's class loader the init finds none of btleplug's Java classes.
    #[test]
    fn the_app_class_loader_is_set_before_btleplug_looks_classes_up() {
        const SELF: &str = include_str!("android_ble.rs");
        let set = SELF.find("use_app_class_loader(env, activity)?;");
        let init = SELF.find("btleplug::platform::init(env)?;");
        assert!(
            matches!((set, init), (Some(set), Some(init)) if set < init),
            "the context class loader must be set before platform::init"
        );
    }
}
