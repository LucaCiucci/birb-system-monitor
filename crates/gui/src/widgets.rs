use egui::Ui;

pub fn placeholder_sentence(ui: &mut Ui) {
    let offset = (*BASE).wrapping_add(ui.time() as u64 / 60);
    ui.label(random_sentence(ui.id().value().wrapping_add(offset)));
}

pub fn random_sentence(k: u64) -> &'static str {
    let idx = k as usize % SENTENCES.len();
    SENTENCES[idx]
}

lazy_static::lazy_static! {
    static ref BASE: u64 = fastrand::u64(0..u64::MAX);
}

const SENTENCES: &[&str] = &[
    "This system is a quantum superposition of 'fine' and 'on fire' — currently decohering into the latter.",
    "The system's entropy is increasing, but according to the Second Law, that's technically not my fault.",
    "CPU, memory, and I/O form a chaotic three-body problem. The numerical integrator is your patience.",
    "Every byte of memory is either allocated, freed, or emotionally unavailable. Rust would like a word.",
    "Your swap file is the event horizon of a virtual memory black hole. Latency goes in; hope does not come out.",
    "All threads are asymptotically free until they hit the strong force of mutex contention.",
    "The uptime counter is the system's proper time. Reboots are just suspicious coordinate transformations.",
    "Disk I/O is friction: negligible in toy models, dominant in reality, and always opposed to progress.",

    "The scheduler has solved the many-body problem by giving up and calling it fairness.",
    "This process tree has more branches than a poorly reviewed Git history.",
    "The kernel is calm. This is not evidence that the situation is under control.",
    "RAM usage is only a number until the OOM killer develops opinions.",
    "The OOM killer is not angry. It is just performing garbage collection with consequences.",
    "One process is leaking memory. Another is leaking confidence.",
    "The fan curve suggests the laptop has entered its turbine era.",
    "Thermal throttling is just the CPU practicing mindfulness.",

    "A static analyzer looked at this system and requested a smaller example.",
    "The dependency graph is acyclic, except emotionally.",
    "The borrow checker cannot save this process. It moved itself into swap.",
    "This mutex has been locked since the Cretaceous period.",
    "Deadlock detected: two threads are waiting for each other to become better people.",
    "The stack is fine. The heap has started writing poetry.",
    "This task is technically running, in the same sense that a PhD is technically progressing.",
    "The logs contain no errors, only increasingly specific warnings from the universe.",

    "Prolog could probably explain this state, but only after allocating 26 GB of RAM.",
    "The system is mostly deterministic, except for drivers, firmware, and vibes.",
    "The GPU is idle, but in a judgmental way.",
    "The CPU load is high because every core is independently rediscovering regret.",
    "This monitor is lightweight, which is more than can be said for some desktop environments.",
    "The process list has been normalized. The underlying suffering has not.",
    "No undefined behavior was observed, but the observer was not sufficiently brave.",
    "The system is stable under small perturbations, unless you touch the USB cable.",
    "Everything is cached, except the one thing you actually needed.",
    "The filesystem is consistent, but only in the legal sense.",
    "A reboot would fix this, but so would confronting the void.",
];
