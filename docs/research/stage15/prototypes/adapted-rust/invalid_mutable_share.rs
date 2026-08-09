// SPDX-License-Identifier: GPL-3.0-or-later

fn main() {
    let mut value = 0u64;
    std::thread::scope(|scope| {
        let first = &mut value;
        scope.spawn(move || *first += 1);
        let second = &mut value;
        *second += 1;
    });
}
