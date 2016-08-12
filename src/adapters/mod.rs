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
#[cfg(feature = "ip_camera")]
mod ip_camera;

/// An adapter dedicated to the Philips Hue
#[cfg(feature = "philips_hue")]
mod philips_hue;

/// An adapter providing access to Thinkerbell.
#[cfg(feature = "thinkerbell")]
mod thinkerbell;

/// An adapter providing `WebPush` services.
#[cfg(feature = "webpush")]
pub mod webpush;

use foxbox_taxonomy::manager::AdapterManager as TaxoManager;

#[cfg(feature = "thinkerbell")]
use self::thinkerbell::ThinkerbellAdapter;
use foxbox_core::traits::Controller;

#[cfg(feature = "zwave")]
use openzwave;

use std::sync::Arc;

#[allow(dead_code)] // workaround for buggy "struct field is never used: `controller`" warning.
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

    #[cfg(feature = "philips_hue")]
    fn start_philips_hue(&self, manager: &Arc<TaxoManager>) {
        philips_hue::PhilipsHueAdapter::init(manager, self.controller.clone()).unwrap();
    }

    #[cfg(not(feature = "philips_hue"))]
    fn start_philips_hue(&self, _: &Arc<TaxoManager>) {
        // nothing to see :)
    }

    #[cfg(feature = "thinkerbell")]
    fn start_thinkerbell(&self, manager: &Arc<TaxoManager>) {
        let scripts_path = &self.controller.get_profile().path_for("thinkerbell_scripts.sqlite");
        ThinkerbellAdapter::init(manager, scripts_path).unwrap(); // FIXME: no unwrap!
    }

    #[cfg(not(feature = "thinkerbell"))]
    fn start_thinkerbell(&self, _: &Arc<TaxoManager>) {
        // nothing to see :)
    }

    #[cfg(feature = "webpush")]
    fn start_webpush(&self, manager: &Arc<TaxoManager>) {
        webpush::WebPush::init(self.controller.clone(), manager).unwrap();
    }

    #[cfg(not(feature = "webpush"))]
    fn start_webpush(&self, _: &Arc<TaxoManager>) {
        // nothing to see :)
    }

    #[cfg(feature = "ip_camera")]
    fn start_ip_camera(&self, manager: &Arc<TaxoManager>) {
        ip_camera::IPCameraAdapter::init(manager, self.controller.clone()).unwrap();
    }

    #[cfg(not(feature = "ip_camera"))]
    fn start_ip_camera(&self, _: &Arc<TaxoManager>) {
        // nothing to see :)
    }

    /// Start all the adapters.
    pub fn start(&mut self, manager: &Arc<TaxoManager>) {
        console::Console::init(manager).unwrap(); // FIXME: We should have a way to report errors
        clock::Clock::init(manager).unwrap(); // FIXME: We should have a way to report errors

        self.start_webpush(manager);
        self.start_ip_camera(manager);
        self.start_thinkerbell(manager);
        self.start_philips_hue(manager);
        self.start_zwave(manager);
        self.start_tts(manager);
    }

    /// Stop all the adapters.
    pub fn stop(&self) {
    }
}
