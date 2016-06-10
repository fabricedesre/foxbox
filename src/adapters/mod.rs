/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/// An adapter providing time services.
pub mod clock;

/// An adapter displaying messages on the console.
pub mod console;

/// A Text To Speak adapter
#[cfg(target_os = "linux")]
pub mod tts;

/// An adapter providing access to IP cameras.
mod ip_camera;

/// An adapter dedicated to the Philips Hue
mod philips_hue;

/// An adapter providing access to Thinkerbell.
mod thinkerbell;

/// An adapter providing `WebPush` services.
pub mod webpush;

use foxbox_taxonomy::manager::AdapterManager as TaxoManager;

use self::thinkerbell::ThinkerbellAdapter;
use foxbox_core::traits::Controller;

#[cfg(feature = "zwave")]
use openzwave;

use std::sync::Arc;

pub struct AdapterManager<T> {
    controller: T,
}

impl<T: Controller> AdapterManager<T> {
    pub fn new(controller: T) -> Self {
        debug!("Creating Adapter Manager");
        AdapterManager {
            controller: controller,
        }
    }

    #[cfg(target_os = "linux")]
    fn start_tts(&self, manager: &Arc<TaxoManager>) {
        tts::init(manager).unwrap();
    }

    #[cfg(not(target_os = "linux"))]
    fn start_tts(&self, _: &Arc<TaxoManager>) {
        info!("No tts support on this platform.");
    }

    #[cfg(feature = "zwave")]
    fn start_zwave(&self, manager: &Arc<TaxoManager>) {
        let profile_openzwave = &self.controller.get_profile().path_for("openzwave");

        let openzwave_devices = self.controller.clone().get_config().get("openzwave", "devices");
        openzwave::Adapter::init(manager, profile_openzwave, openzwave_devices).unwrap(); // FIXME convert to a local error
    }

    #[cfg(not(feature = "zwave"))]
    fn start_zwave(&self, _: &Arc<TaxoManager>) {
        // nothing to see :)
    }

    /// Start all the adapters.
    pub fn start(&mut self, manager: &Arc<TaxoManager>) {
        let c = self.controller.clone(); // extracted here to prevent double-borrow of 'self'
        console::Console::init(manager).unwrap(); // FIXME: We should have a way to report errors
        philips_hue::PhilipsHueAdapter::init(manager, c.clone()).unwrap();
        clock::Clock::init(manager).unwrap(); // FIXME: We should have a way to report errors
        webpush::WebPush::init(c, manager).unwrap();
        ip_camera::IPCameraAdapter::init(manager, self.controller.clone()).unwrap();
        let scripts_path = &self.controller.get_profile().path_for("thinkerbell_scripts.sqlite");
        ThinkerbellAdapter::init(manager, scripts_path).unwrap(); // FIXME: no unwrap!

        self.start_zwave(manager);
        self.start_tts(manager);
    }

    /// Stop all the adapters.
    pub fn stop(&self) {
    }
}
