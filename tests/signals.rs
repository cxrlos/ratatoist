use std::process::Command;
use std::time::Duration;

use ratatoist::terminal;

// One test for all signals: every shutdown future in the process sees every
// signal, so separate tests running in parallel would pass on each other's.
#[tokio::test]
async fn interrupt_terminate_and_hangup_each_ask_ratatoist_to_shut_down() {
    for signal in ["INT", "TERM", "HUP"] {
        let shutdown = terminal::shutdown_signal().unwrap();

        // As `kill -INT <pid>` from a shell would.
        let sent = Command::new("kill")
            .arg(format!("-{signal}"))
            .arg(std::process::id().to_string())
            .status()
            .unwrap();

        assert!(sent.success(), "kill -{signal} failed");
        assert!(
            tokio::time::timeout(Duration::from_secs(2), shutdown)
                .await
                .is_ok(),
            "SIG{signal} did not resolve the shutdown future"
        );
    }
}
