use nix::sys::signal::Signal;

pub struct SignalChoice {
    pub signal: Signal,
    pub label: &'static str,
}

pub const SIGNAL_CHOICES: &[SignalChoice] = &[
    SignalChoice {
        signal: Signal::SIGTERM,
        label: "TERM",
    },
    SignalChoice {
        signal: Signal::SIGKILL,
        label: "KILL",
    },
    SignalChoice {
        signal: Signal::SIGHUP,
        label: "HUP",
    },
    SignalChoice {
        signal: Signal::SIGINT,
        label: "INT",
    },
    SignalChoice {
        signal: Signal::SIGUSR1,
        label: "USR1",
    },
    SignalChoice {
        signal: Signal::SIGUSR2,
        label: "USR2",
    },
    SignalChoice {
        signal: Signal::SIGSTOP,
        label: "STOP",
    },
    SignalChoice {
        signal: Signal::SIGCONT,
        label: "CONT",
    },
];
