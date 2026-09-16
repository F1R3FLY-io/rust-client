//! The client's integration suite: every test runs against a real shard that
//! this harness creates and destroys.
//!
//!   cargo test --features integration --test integration
//!
//! Without the feature the target does not build, so a missing shard can never
//! read as a pass. `FIREFLY_KEEP_SHARD=1` leaves the shard up for debugging.

mod common;
mod deploys;
mod routing;
mod transfers;

use std::path::Path;
use std::process::ExitCode;

use libtest_mimic::{Arguments, Trial};

use common::shard::Shard;

fn main() -> ExitCode {
    let mut args = Arguments::from_args();

    let parallel: Vec<Trial> = routing::trials()
        .into_iter()
        .chain(deploys::trials())
        .chain(transfers::trials())
        .collect();
    let serial: Vec<Trial> = Vec::new();

    if parallel.is_empty() && serial.is_empty() {
        eprintln!("no tests were collected");
        return ExitCode::FAILURE;
    }

    // Listing must not pay for a shard.
    if args.list {
        libtest_mimic::run(&args, parallel.into_iter().chain(serial).collect()).exit();
    }

    let shard = match Shard::start() {
        Ok(shard) => shard,
        Err(e) => {
            eprintln!("could not start the shard: {e}");
            return ExitCode::FAILURE;
        }
    };

    let parallel_result = libtest_mimic::run(&args, parallel);

    // The serial group shares shard-wide state - the bond set, the active
    // validators - so it cannot run alongside anything else.
    args.test_threads = Some(1);
    let serial_result = libtest_mimic::run(&args, serial);

    let failed = parallel_result.has_failed() || serial_result.has_failed();
    if failed {
        // A node that died takes every test that talks to it down with it, so
        // say so before the individual failures are read as the cause.
        if let Ok(Some(exited)) = shard.exited_container() {
            eprintln!("\n{exited} is no longer running - the failures above follow from that");
        }
        let logs = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/integration-logs");
        match shard.dump_logs(&logs) {
            Ok(()) => eprintln!("container logs written to {}", logs.display()),
            Err(e) => eprintln!("could not write container logs: {e}"),
        }
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
