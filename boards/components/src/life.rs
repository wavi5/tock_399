// Licensed under the Apache License, Version 2.0 or the MIT License.
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Copyright Tock Contributors 2022.

//! Components for the LifeDriver.
//!
//! Usage
//! -----
//! ```rust
//! let life = components::life::LifeComponent::new().finalize(());
//! ```

use capsules_core::life::LifeDriver;
use core::marker::PhantomData;

use kernel::component::Component;

#[macro_export]
macro_rules! life_component_static {
    () => {{
        let life = kernel::static_init!(LifeDriver, LifeDriver::new());
        life
    }};
}

pub struct LifeComponent {
    _phantom: PhantomData<LifeDriver>,
}

impl LifeComponent {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl Component for LifeComponent {
    type StaticInput = ();
    type Output = &'static LifeDriver;

    fn finalize(self, _static_buffer: Self::StaticInput) -> Self::Output {
        let life = unsafe { life_component_static!() };
        life
    }
}
