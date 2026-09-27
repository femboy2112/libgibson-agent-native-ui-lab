use std::process::Command as ProcessCommand;
use std::time::Duration;

use gibson::capability::ColorDepth;
use gibson::ui::prelude::*;
use gibson::{Context, RenderMode};

use project_galileo::demo::DemoScript;
use project_galileo::render::scale::{PrimaryView, ScaleCoordinator};
use project_galileo::sim::jovian::JovianModel;
use project_galileo::sim::telemetry::{MAX_ALERT_LOGS, MAX_HISTORY_SAMPLES};
use project_galileo::sim::trajectory::TrajectoryLab;
use project_galileo::ui::command::{Command, CommandParser, ViewTarget, ZoomAction};
use project_galileo::ui::dashboard::{ActiveView, DashboardState};

const REQUIRED_DIMENSIONS: &[(u16, u16)] = &[(160, 50), (120, 40), (100, 30), (80, 24), (60, 20)];

const COLOR_DEPTHS: &[ColorDepth] = &[
    ColorDepth::TrueColor,
    ColorDepth::Ansi256,
    ColorDepth::Ansi16,
    ColorDepth::Mono,
];

const VIEWS: &[ActiveView] = &[
    ActiveView::System,
    ActiveView::Trajectory,
    ActiveView::Surface,
    ActiveView::Tomography,
    ActiveView::MissionControl,
];

#[test]
fn test_all_views_render_across_all_required_dimensions() {
    for &(w, h) in REQUIRED_DIMENSIONS {
        for &view in VIEWS {
            let mut state = DashboardState::new(false);
            state.active_view = view;

            let mut context = Context::headless(RenderMode::Fullscreen, w, h);
            context.set_color_depth(ColorDepth::TrueColor);

            let mut runtime = UiRuntime::new(state.resolved_skin());
            let env = UiEnvironment {
                width: w,
                height: h,
                color_depth: ColorDepth::TrueColor,
                ..Default::default()
            };

            let now = Duration::from_millis(1000);
            let cx = runtime.build_cx(env, now);
            let tree = state.view(&cx);

            let frame = runtime.frame(&tree, env, now).unwrap_or_else(|e| {
                panic!("Frame failed for view {:?} at {}x{}: {:?}", view, w, h, e)
            });

            context.set_root(frame.node);
            context.render_now().unwrap_or_else(|e| {
                panic!("Render failed for view {:?} at {}x{}: {:?}", view, w, h, e)
            });

            let bytes = context.rendered_bytes();
            assert!(
                !bytes.is_empty(),
                "Bytes should not be empty for view {:?} at {}x{}",
                view,
                w,
                h
            );
        }
    }
}

#[test]
fn test_all_views_render_across_all_color_depths() {
    let (w, h) = (120, 40);
    for &depth in COLOR_DEPTHS {
        for &view in VIEWS {
            let mono = depth == ColorDepth::Mono;
            let mut state = DashboardState::new(mono);
            state.active_view = view;

            let mut context = Context::headless(RenderMode::Fullscreen, w, h);
            context.set_color_depth(depth);

            let mut runtime = UiRuntime::new(state.resolved_skin());
            let env = UiEnvironment {
                width: w,
                height: h,
                color_depth: depth,
                ..Default::default()
            };

            let now = Duration::from_millis(2500);
            let cx = runtime.build_cx(env, now);
            let tree = state.view(&cx);

            let frame = runtime.frame(&tree, env, now).unwrap_or_else(|e| {
                panic!(
                    "Frame failed for view {:?} with depth {:?}: {:?}",
                    view, depth, e
                )
            });

            context.set_root(frame.node);
            context.render_now().unwrap_or_else(|e| {
                panic!(
                    "Render failed for view {:?} with depth {:?}: {:?}",
                    view, depth, e
                )
            });

            let bytes = context.rendered_bytes();
            assert!(!bytes.is_empty());
        }
    }
}

#[test]
fn test_deterministic_capture_reproducibility() {
    let (w, h) = (120, 40);
    let at_ms = 7500;
    let demo = DemoScript::new();

    let render_frame = || -> Vec<u8> {
        let mut state = DashboardState::new(false);
        demo.apply_at_ms(&mut state, at_ms);

        let mut context = Context::headless(RenderMode::Fullscreen, w, h);
        context.set_color_depth(ColorDepth::TrueColor);

        let mut runtime = UiRuntime::new(state.resolved_skin());
        let env = UiEnvironment {
            width: w,
            height: h,
            color_depth: ColorDepth::TrueColor,
            ..Default::default()
        };

        let now = Duration::from_millis(at_ms);
        let cx = runtime.build_cx(env, now);
        let tree = state.view(&cx);
        let frame = runtime.frame(&tree, env, now).expect("Failed frame");

        context.set_root(frame.node);
        context.render_now().expect("Failed render");
        context.rendered_bytes().to_vec()
    };

    let run1 = render_frame();
    let run2 = render_frame();

    assert_eq!(
        run1, run2,
        "Deterministic frames at exact same millisecond must be bit-for-bit identical!"
    );
}

#[test]
fn test_bounded_telemetry_buffers() {
    let mut state = DashboardState::new(false);

    // Simulate 500 seconds of mission elapsed time
    for _ in 0..500 {
        state.update(1.0);
    }

    // Verify bounded memory invariants
    assert!(state.telemetry.rtg_history.len() <= MAX_HISTORY_SAMPLES);
    assert!(state.telemetry.snr_history.len() <= MAX_HISTORY_SAMPLES);
    assert!(state.telemetry.thermal_history.len() <= MAX_HISTORY_SAMPLES);
    assert!(state.telemetry.alerts.len() <= MAX_ALERT_LOGS);
    assert!(state.console_history.len() <= 16);
}

#[test]
fn test_scale_coordinator_transitions() {
    let mut coordinator = ScaleCoordinator::default();
    assert_eq!(coordinator.current_view, PrimaryView::System);
    assert_eq!(coordinator.zoom_level, 0.0);

    // Zoom into Trajectory
    coordinator.set_view(PrimaryView::Trajectory);
    assert!(coordinator.is_transitioning);

    // Step transition time
    coordinator.update(1.5);
    assert!(!coordinator.is_transitioning);
    assert_eq!(coordinator.current_view, PrimaryView::Trajectory);
    assert!((coordinator.zoom_level - PrimaryView::Trajectory.target_scale()).abs() < 0.01);

    // Zoom into Tomography
    coordinator.set_view(PrimaryView::Tomography);
    coordinator.update(1.5);
    assert_eq!(coordinator.current_view, PrimaryView::Tomography);
    assert!((coordinator.zoom_level - PrimaryView::Tomography.target_scale()).abs() < 0.01);
}

#[test]
fn test_trajectory_maneuver_recomputation() {
    let sim = JovianModel::default();
    let mut lab = TrajectoryLab::new(&sim);

    let initial_ca = lab.closest_approach_nominal.altitude_km;
    assert!(initial_ca > 0.0);

    // Add burn of 50 m/s prograde
    lab.add_node(sim.mission_time_hours + 10.0);
    if let Some(node) = lab.selected_node_mut() {
        node.dv_prograde = 50.0;
    }
    lab.recompute(&sim);

    let planned_ca = lab.closest_approach_planned.altitude_km;
    assert_ne!(
        initial_ca, planned_ca,
        "Planned closest approach should differ after maneuver burn"
    );
}

#[test]
fn test_command_parser() {
    assert_eq!(
        CommandParser::parse("view 4"),
        Some(Command::SetView(ViewTarget::Tomography))
    );
    assert_eq!(
        CommandParser::parse("zoom in"),
        Some(Command::Zoom(ZoomAction::In))
    );
    assert_eq!(
        CommandParser::parse("target europa"),
        Some(Command::SetTarget("europa".to_string()))
    );
    assert_eq!(
        CommandParser::parse("burn 45.5"),
        Some(Command::ExecuteBurn(45.5))
    );
    assert_eq!(
        CommandParser::parse("alert ack"),
        Some(Command::AcknowledgeAlert)
    );
    assert_eq!(CommandParser::parse("help"), Some(Command::Help));
}

#[test]
fn test_screen_content_verification() {
    let (w, h) = (120, 40);
    for &view in VIEWS {
        let mut state = DashboardState::new(false);
        state.active_view = view;

        let mut context = Context::headless(RenderMode::Fullscreen, w, h);
        context.set_color_depth(ColorDepth::TrueColor);

        let mut runtime = UiRuntime::new(state.resolved_skin());
        let env = UiEnvironment {
            width: w,
            height: h,
            color_depth: ColorDepth::TrueColor,
            motion: gibson::ui::MotionPreference::None,
            ..Default::default()
        };

        let now = Duration::from_millis(1000);
        let cx = runtime.build_cx(env, now);
        let tree = state.view(&cx);

        let frame = runtime.frame(&tree, env, now).unwrap();
        context.set_root(frame.node);
        context.render_now().unwrap();

        let bytes = context.rendered_bytes();

        let mut parser = vt100::Parser::new(h, w, 0);
        parser.process(bytes);
        let screen = parser.screen();

        let mut row0 = String::new();
        for r in 0..15 {
            let mut row = String::new();
            for col in 0..w {
                row.push(
                    screen
                        .cell(r, col)
                        .map(|c| c.contents().chars().next().unwrap_or(' '))
                        .unwrap_or(' '),
                );
            }
            if r == 0 {
                row0 = row.clone();
            }
            if !row.trim().is_empty() {
                println!("VIEW {:?} ROW {:02}: '{}'", view, r, row);
            }
        }

        // Header must contain title
        assert!(
            row0.contains("PROJECT GALILEO"),
            "Row 0 should contain title, got: {}",
            row0
        );

        // Header must contain view button for this view
        match view {
            ActiveView::System => assert!(row0.contains("1:SYS")),
            ActiveView::Trajectory => assert!(row0.contains("2:TRAJ")),
            ActiveView::Surface => assert!(row0.contains("3:SURF")),
            ActiveView::Tomography => assert!(row0.contains("4:TOMO")),
            ActiveView::MissionControl => assert!(row0.contains("5:OPS")),
        }
    }
}

#[test]
fn test_expanded_command_parser() {
    assert_eq!(
        CommandParser::parse("warp 60"),
        Some(Command::SetWarpRate(60.0))
    );
    assert_eq!(CommandParser::parse("pause"), Some(Command::Pause));
    assert_eq!(CommandParser::parse("resume"), Some(Command::Resume));
    assert_eq!(
        CommandParser::parse("gate 14.5"),
        Some(Command::SetGate(14.5))
    );
    assert_eq!(
        CommandParser::parse("gain 48"),
        Some(Command::SetGain(48.0))
    );
    assert_eq!(
        CommandParser::parse("rotate 215.0 -20.0"),
        Some(Command::Rotate(215.0, -20.0))
    );
    assert_eq!(
        CommandParser::parse("scrub 72.0"),
        Some(Command::Scrub(72.0))
    );
    assert_eq!(
        CommandParser::parse("thermal"),
        Some(Command::ToggleThermal)
    );
    assert_eq!(CommandParser::parse("mag"), Some(Command::ToggleMagnetic));
    assert_eq!(
        CommandParser::parse("band split"),
        Some(Command::SetRadarBand(
            project_galileo::render::tomography_view::RadarBand::SplitBand
        ))
    );
}

#[test]
fn test_coupled_subsystems_simulation() {
    let mut state = DashboardState::new(false);

    // Initial battery SoC and propellant mass
    let init_soc = state.telemetry.battery_soc;
    let init_fuel = state.telemetry.main_engine_propellant_kg;

    // Simulate 300 seconds
    for _ in 0..300 {
        state.update(1.0);
    }

    // Execute a maneuver burn
    state.execute_command(Command::ExecuteBurn(45.0));

    // Verify propellant was deducted
    assert!(
        state.telemetry.main_engine_propellant_kg < init_fuel,
        "Propellant should be deducted after burn"
    );

    // Verify battery SoC remains bounded between 0.05 and 1.0
    assert!(state.telemetry.battery_soc >= 0.05 && state.telemetry.battery_soc <= 1.0);
    let _ = init_soc;
}

#[test]
fn test_b_plane_deflection_calculation() {
    let sim = JovianModel::default();
    let mut lab = TrajectoryLab::new(&sim);

    let nom_bt = lab.closest_approach_nominal.b_dot_t_km;
    let nom_br = lab.closest_approach_nominal.b_dot_r_km;

    // Adjust prograde and normal burn
    lab.adjust_selected_node(25.0, 10.0, 5.0, &sim);

    let plan_bt = lab.closest_approach_planned.b_dot_t_km;
    let plan_br = lab.closest_approach_planned.b_dot_r_km;

    assert!(nom_bt.is_finite());
    assert!(nom_br.is_finite());
    assert!(plan_bt.is_finite());
    assert!(plan_br.is_finite());
    assert_ne!(nom_bt, plan_bt, "B.T should change after burn perturbation");
}

#[test]
fn test_continuous_zoom_scaling_and_view_transitions() {
    let mut state = DashboardState::new(false);
    assert_eq!(state.active_view, ActiveView::System);
    assert_eq!(state.system_renderer.camera_distance_scale, 1.0);

    // 1. Zoom in within SystemView
    state.execute_command(Command::Zoom(ZoomAction::In));
    assert!(state.system_renderer.camera_distance_scale > 1.0);
    assert_eq!(state.active_view, ActiveView::System);

    // Keep zooming until scale transition to TrajectoryView
    for _ in 0..10 {
        state.execute_command(Command::Zoom(ZoomAction::In));
        if state.active_view == ActiveView::Trajectory {
            break;
        }
    }
    assert_eq!(
        state.active_view,
        ActiveView::Trajectory,
        "Should descend into TrajectoryView"
    );
    assert!(state
        .scale_coordinator
        .physical_scale_label()
        .contains("ORBITAL REGIME"));

    // Keep zooming until scale transition to SurfaceView
    for _ in 0..10 {
        state.execute_command(Command::Zoom(ZoomAction::In));
        if state.active_view == ActiveView::Surface {
            break;
        }
    }
    assert_eq!(
        state.active_view,
        ActiveView::Surface,
        "Should descend into SurfaceView"
    );
    assert!(state
        .scale_coordinator
        .physical_scale_label()
        .contains("EUROPA SURFACE"));

    // Keep zooming until scale transition to TomographyView
    for _ in 0..10 {
        state.execute_command(Command::Zoom(ZoomAction::In));
        if state.active_view == ActiveView::Tomography {
            break;
        }
    }
    assert_eq!(
        state.active_view,
        ActiveView::Tomography,
        "Should descend into TomographyView"
    );
    assert!(
        state
            .scale_coordinator
            .physical_scale_label()
            .contains("ICE SHELL")
            || state
                .scale_coordinator
                .physical_scale_label()
                .contains("OCEAN")
    );

    // 2. Now zoom OUT and verify ascending transitions
    for _ in 0..15 {
        state.execute_command(Command::Zoom(ZoomAction::Out));
        if state.active_view == ActiveView::Surface {
            break;
        }
    }
    assert_eq!(
        state.active_view,
        ActiveView::Surface,
        "Should ascend into SurfaceView"
    );

    for _ in 0..15 {
        state.execute_command(Command::Zoom(ZoomAction::Out));
        if state.active_view == ActiveView::Trajectory {
            break;
        }
    }
    assert_eq!(
        state.active_view,
        ActiveView::Trajectory,
        "Should ascend into TrajectoryView"
    );

    for _ in 0..15 {
        state.execute_command(Command::Zoom(ZoomAction::Out));
        if state.active_view == ActiveView::System {
            break;
        }
    }
    assert_eq!(
        state.active_view,
        ActiveView::System,
        "Should ascend into SystemView"
    );
}

#[test]
fn test_renderer_zoom_surfaces() {
    let mut surf = project_galileo::render::surface_view::SurfaceViewRenderer::default();
    surf.adjust_zoom(0.5);
    let s1 = surf.render(80, 24, false);
    assert_eq!(s1.width, 80);
    assert_eq!(s1.height, 24);

    surf.adjust_zoom(4.0);
    let s2 = surf.render(80, 24, false);
    assert_eq!(s2.width, 80);
    assert_eq!(s2.height, 24);

    let mut tomo = project_galileo::render::tomography_view::TomographyViewRenderer::default();
    tomo.adjust_zoom(0.5);
    let t1 = tomo.render(80, 24, false);
    assert_eq!(t1.width, 80);
    assert_eq!(t1.height, 24);

    tomo.adjust_zoom(4.0);
    let t2 = tomo.render(80, 24, false);
    assert_eq!(t2.width, 80);
    assert_eq!(t2.height, 24);
}

#[test]
fn test_radar_gate_survives_ticks_and_remains_visible_in_vhf() {
    let mut state = DashboardState::new(false);
    state.execute_command(Command::SetGate(1.5));
    for _ in 0..30 {
        state.update(1.0 / 30.0);
    }
    assert_eq!(state.tomography_renderer.cursor_depth_km, 1.5);

    state.execute_command(Command::SetRadarBand(
        project_galileo::render::tomography_view::RadarBand::Vhf60MHz,
    ));
    state.execute_command(Command::SetGate(35.0));
    assert_eq!(state.tomography_renderer.cursor_depth_km, 7.0);
    let radar = state.tomography_renderer.render(80, 24, false);
    let mut text = String::new();
    for y in 0..radar.height {
        for x in 0..radar.width {
            text.push_str(radar.get(x, y).unwrap().glyph.grapheme.as_str());
        }
    }
    assert!(text.contains("GATE  7.0km"), "VHF gate diagnostics absent");
    assert!(text.contains("BASE:"), "Basal offset diagnostics absent");
}

#[test]
fn test_selected_site_reaches_deterministic_radar_capture() {
    let mut state = DashboardState::new(false);
    DemoScript::new().apply_at_ms(&mut state, 13_000);
    assert_eq!(state.active_view, ActiveView::Tomography);
    assert_eq!(state.tomography_renderer.feature_name, "CONAMARA CHAOS");
    assert_eq!(state.tomography_renderer.nominal_ice_thickness_km, 14.2);
}

#[test]
fn test_scale_reveal_keeps_previous_scene_then_completes() {
    let mut state = DashboardState::new(false);
    state.active_view = ActiveView::Surface;
    state.surface_renderer.zoom = 2.3;
    state.execute_command(Command::Zoom(ZoomAction::In));
    assert_eq!(state.active_view, ActiveView::Tomography);
    let previous = state.surface_renderer.render(80, 24, false);
    let opening = state.render_visualization_surface(80, 24);
    assert!(
        *opening == previous,
        "reveal should begin with previous scene"
    );
    state.update(0.3);
    let middle = state.render_visualization_surface(80, 24);
    assert!(
        *middle != previous,
        "mid-scale frame should expose radar data"
    );
    state.update(0.4);
    let final_radar = state.tomography_renderer.render(80, 24, false);
    assert!(*state.render_visualization_surface(80, 24) == final_radar);
}

#[test]
fn test_illustrative_encounter_is_inside_europa_sphere_of_influence() {
    let model = JovianModel::default();
    let lab = TrajectoryLab::new(&model);
    assert!(lab.closest_approach_nominal.altitude_km > 0.0);
    assert!(lab.closest_approach_nominal.distance_km < 9_700.0);
    assert!(lab.closest_approach_planned.distance_km.is_finite());
}

#[test]
fn test_orbital_target_and_encounter_handoff_agree() {
    let mut state = DashboardState::new(false);
    state.execute_command(Command::SetTarget("io".to_string()));
    assert_eq!(state.system_renderer.selected_target_name(), "IO");
    assert_eq!(
        state.sim.target,
        project_galileo::sim::jovian::TargetBody::Io
    );
    state.system_renderer.camera_distance_scale = 2.7;
    state.execute_command(Command::Zoom(ZoomAction::In));
    assert_eq!(state.active_view, ActiveView::System);
    assert!(state.status_feedback.contains("SELECT EUROPA"));
    state.execute_command(Command::SetTarget("europa".to_string()));
    state.execute_command(Command::Zoom(ZoomAction::In));
    assert_eq!(state.active_view, ActiveView::Trajectory);
}

#[test]
fn test_demo_transitions_keep_shell_and_instrument_readouts_whole() {
    let demo = DemoScript::new();
    for ms in [4_200, 4_400, 4_650, 8_200, 8_400, 12_200, 12_400] {
        let mut state = DashboardState::new(false);
        demo.apply_at_ms(&mut state, ms);
        let (width, height) = (120, 40);
        let env = UiEnvironment {
            width,
            height,
            color_depth: ColorDepth::TrueColor,
            motion: MotionPreference::None,
            ..Default::default()
        };
        let mut runtime = UiRuntime::new(state.resolved_skin());
        let now = Duration::from_millis(ms);
        let tree = state.view(&runtime.build_cx(env, now));
        let frame = runtime.frame(&tree, env, now).unwrap();
        let mut context = Context::headless(RenderMode::Fullscreen, width, height);
        context.set_root(frame.node);
        context.render_now().unwrap();
        let mut parser = vt100::Parser::new(height, width, 0);
        parser.process(context.rendered_bytes());
        let screen = parser.screen();
        let row = |r: u16| -> String {
            (0..width)
                .map(|c| {
                    screen
                        .cell(r, c)
                        .map_or(" ", |cell| cell.contents())
                        .to_string()
                })
                .collect()
        };
        let body = (0..height).map(row).collect::<Vec<_>>().join("\n");
        assert!(row(0).contains("ALERTS:"), "top shell absent at {ms}ms");
        assert!(body.contains("EPS TELEMETRY"), "rail absent at {ms}ms");
        assert!(body.contains("CMD>"), "command bar absent at {ms}ms");
        assert!(body.contains("[RUN]"), "status absent at {ms}ms");
        if ms == 4_200 {
            assert!(
                !body.contains("NC: E-"),
                "fragmented destination text at {ms}ms"
            );
        } else if ms == 4_400 {
            assert!(
                body.contains("TRAJECTORY LAB // FLYBY DYNAMICS"),
                "destination HUD incomplete at {ms}ms"
            );
        } else if ms == 8_200 {
            assert!(
                !body.contains("MORPHOLOGY:") && !body.contains("PAYLOAD STATU"),
                "surface inspector should await an atomic handoff at {ms}ms"
            );
        } else if ms == 8_400 {
            assert!(
                body.contains("EUROPA REGIONAL SURVEY") && body.contains("MORPHOLOGY:"),
                "surface inspector incomplete at {ms}ms"
            );
        } else if ms == 12_200 {
            assert!(
                body.contains("EUROPA REGIONAL SURVEY") && !body.contains("RADAR A-SCAN ECHO"),
                "surface inspector should remain whole at {ms}ms"
            );
        } else if ms == 12_400 {
            assert!(
                body.contains("RADAR A-SCAN ECHO") && body.contains("GATE"),
                "radar diagnostics should arrive whole at {ms}ms"
            );
        }
    }
}

#[test]
fn test_profiler_counts_style_only_changes() {
    let mut profiler = project_galileo::profile::Profiler::new(4, 1);
    profiler.record_frame(
        b"\x1b[1;1H\x1b[38;2;255;0;0mA",
        Duration::from_millis(1),
        0,
        0,
    );
    let changed = profiler.record_frame(
        b"\x1b[1;1H\x1b[38;2;0;255;0mA",
        Duration::from_millis(1),
        0,
        0,
    );
    assert_eq!(changed.changed_cells, 1);
}

#[test]
fn test_headless_profiler_counts_incremental_output() {
    let result = ProcessCommand::new(env!("CARGO_BIN_EXE_project-galileo"))
        .args([
            "--demo",
            "--headless",
            "--frames=40",
            "--fps=60",
            "--width=120",
            "--height=40",
            "--profile",
        ])
        .output()
        .expect("headless demo executable");
    assert!(result.status.success());
    let report = String::from_utf8(result.stderr).expect("profiler output");
    assert!(report.contains("Frames: 40"), "{report}");
    let avg_bytes = report
        .split("Avg Bytes: ")
        .nth(1)
        .and_then(|s| s.split(" B").next())
        .and_then(|s| s.parse::<usize>().ok())
        .expect("incremental average bytes");
    assert!(
        (1..50_000).contains(&avg_bytes),
        "output should reflect a single 120x40 frame, not the accumulated capture: {report}"
    );
}

#[test]
fn test_compact_scale_handoffs_are_atomic() {
    let demo = DemoScript::new();
    for ms in [8_200, 8_400, 12_200, 12_400] {
        let mut state = DashboardState::new(true);
        demo.apply_at_ms(&mut state, ms);
        let drawn = state.render_visualization_surface(80, 18);
        let expected = match ms {
            8_200 => {
                state
                    .trajectory_renderer
                    .render(&state.sim, &state.trajectory_lab, 80, 18, true)
            }
            8_400 | 12_200 => state.surface_renderer.render(80, 18, true),
            _ => state.tomography_renderer.render(80, 18, true),
        };
        assert!(*drawn == expected, "partial compact scale view at {ms}ms");
    }
}
