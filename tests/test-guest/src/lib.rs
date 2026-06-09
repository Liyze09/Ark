use ark_extension_api::{ExtensionInitializer, core::logging::info, init_logger, initialize};

struct Test;

impl ExtensionInitializer for Test {
    fn on_init() {
        init_logger();
        info("Hello, World!");
    }

    fn on_callback(_id: String) {
        
    }

    fn on_destroy() {

    }
}

initialize!(Test);
