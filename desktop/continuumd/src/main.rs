//! Continuum daemon (headless). UIs talk to it over D-Bus.

fn main() {
    println!(
        "continuumd {} (protocol v{})",
        env!("CARGO_PKG_VERSION"),
        continuum_proto::PROTOCOL_VERSION
    );
}
