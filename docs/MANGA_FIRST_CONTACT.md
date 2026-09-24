# Manga first contact

Public substrate e5ede0a; no core changes. One ordinary goal panel establishes
the page; concurrent narrow worker panels share gutters; the final synthesis
replaces those columns with a full-width splash. Narrow terminals reflow into
two rows. Selection uses a double/reversed frame, explicit index and name, so
Mono does not need color to express reading order or ownership.

**Unplanned device:** the causal repair seam. A tool failure tears across the
gutter; successful recovery stitches it with animated cross-marks. This was
invented during the first implementation after the protocol commit, not copied
from the dossier. It uses public `Surface::print_str` over ordinarily painted
panels. It is an within-experiment escape-hatch probe, not a post-freeze holdout.

F-A1 — keyboard selection across relayout. Desired persistent panel identity.
Attempt: public FocusId/FocusRing plus app `match KeyCode`. Result: stable IDs
survive changing rectangles. Workaround: about 30 lines map IDs to semantic
Select/ResolvePermission actions. **ERGONOMIC INCONVENIENCE**, not an observed
wall. FocusRing deliberately does not decide domain actions.

F-A2 — gutter disruption/repair not representable as a standard panel.
Attempt: ordinary Nodes locally realized and Surface overlay. Result: works
without raw terminal output. About 35 lines of seam drawing. **HARNESS-SPECIFIC**;
no reason to give core a comic gutter or repair-seam type.

F-A3 — small terminal reflow. Manual responsive rectangle policy is needed;
Node layout handles the content inside each panel. Policy ~25 lines. Preserve
semantic FocusId independent of these rectangles. **ERGONOMIC INCONVENIENCE**.

Initial probes caught two consumer mistakes: a 12-row fallback cutoff allowed
unsigned subtraction underflow, and a label assertion sampled an in-progress
dissolve. Corrected consumer minimum height to 18 and sampled after reveal.
No LibGibson failure inferred from either. Fixture permission timing was
aligned before first-contact completion; later experiments share that fixture.

Validation: semantic fixture six tests; manga three tests covering the complete
4-size × 3-capability matrix, repeated/replayed frames, 0 exact/affected/wire
for frozen output, explicit permission denial surviving the scripted default,
selection, repair and bounded hostile dimensions. Reconstructed TrueColor and
narrow Mono were inspected. This is a working research comic, not publication
grade manga art or a claim about arbitrary graphical interfaces.

Real debug-binary PTY checks passed at 56×24 Mono,120×32 TrueColor,160×40
ANSI16: pause,Tab,N,resize to56×24,Esc; exit0 and termios/alternate-screen restored.
