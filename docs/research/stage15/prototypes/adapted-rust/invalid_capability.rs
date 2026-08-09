// SPDX-License-Identifier: GPL-3.0-or-later

mod profile {
    pub struct MmioCapability {
        grant: &'static str,
    }
}

fn main() {
    let _forged = profile::MmioCapability { grant: "device0" };
}
