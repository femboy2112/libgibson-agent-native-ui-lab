use project_chronoscope::fixture::*;
use project_chronoscope::history::*;
fn spans(h:&History,b:BranchId)->String{ let tl=h.timeline(b); let mut out=String::new(); let mut start=0; for i in 1..=tl.len(){ if i==tl.len() || tl[i].epoch!=tl[start].epoch { out+=&format!("{}[{}-{}] ", tl[start].epoch.letter(), start, i-1); start=i; } } out }
fn main() {
    let mut h = History::new(colony(), DEMO_SEED, demo_script());
    h.run_to_end(0);
    println!("root {:?}@{}: {}", h.branch(0).terminal, h.branch(0).end(), spans(&h,0));
    for (at, e) in [(50, Edit::ReplaceCmd(2)), (62, Edit::DropCmd), (74, Edit::DropCmd), (50, Edit::DropCmd), (74, Edit::ReplaceCmd(2))] {
        let b = h.fork(0, at, e.clone()).unwrap(); h.run_to_end(b);
        println!("{:?}@{at} {:?}@{}: {}", e, h.branch(b).terminal, h.branch(b).end(), spans(&h,b));
    }
    let lm = landmarks(&h, 0);
    for l in lm.iter().filter(|l| matches!(l.kind, LandmarkKind::Fate|LandmarkKind::Event|LandmarkKind::Input|LandmarkKind::Catastrophe)).take(40) { println!("  {:4} {:?} {}", l.pos, l.kind, l.text); }
}
