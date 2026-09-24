# Results

## Planned consumers — before API freeze

**OBSERVED:** manga, semantic reactions and generated instruments run against
LibGibson `e5ede0a4ab8ff52caa567ee10d45824e42c1ebc6` using only public APIs.
No LibGibson source changes or private access were needed. Consumer bugs were
found and corrected before freeze; "first contact" does not mean first draft
was bug-free. See each first-contact record and the reconciliation.

Local Rust 1.98.1: fmt, strict all-target Clippy, release bins and 32 integration
tests pass. The tests include one cross-consumer full-Snapshot comparison with
and without explicit denial, all four required sizes, three capabilities,
replayed full input/focus/frames, atomic IR rejection, and frozen 0/0/0 damage.
Nine release PTY runs (each consumer at 56×24 Mono, 120×32 TrueColor,
160×40 ANSI16) exit 0 after pause/Tab/N/resize/Esc, restore termios and leave the
alternate screen. A separate VT100 reconstruction test proves manga selection
and denial reached the actual displayed frame, not merely process exit.

Root inspected reconstructed TrueColor reaction and instrument frames as well
as the earlier manga capture. This is visual legibility evidence on this host,
not an independent human aesthetic assessment or a portability guarantee.
The inspection font initially lacked Braille; explicitly plotting Unicode
Braille bits corrected the scratch viewer. No generated captures are committed.

Shared fixture/model provenance limits independence. No universal "easy" or
arbitrary-runtime-UI claim is made. Long-session resource behavior, mouse,
embedded lifecycle and non-Linux terminals are outside this finite campaign.

## Completed post-freeze campaign

Foldroom, Cuebox and Weavebench meet their finite contracts using the frozen
public API. Selection exclusions, context limits, consumer failures/corrections,
61-test final inventory and verdict boundaries are in the
[final report](AGENT_NATIVE_UI_FINAL_REPORT.md). No core promotion or helper/IR
rescue occurred. The 32-test result above is the historical pre-freeze checkpoint.
