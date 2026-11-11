// SPDX-License-Identifier: AGPL-3.0-or-later
//! CAD entegrasyonu, profil uretimi ve FreeCAD 1.1 betik olusturucu modulleri.

pub mod airfoil;
pub mod freecad;
pub mod geom;
pub mod layout;
pub mod outputs;
pub mod runner;

pub use airfoil::{AirfoilProfile, generate_naca4_points, parse_selig_dat};
pub use freecad::{
    ExportOptions, FuselageCadParams, MotorMountCadParams, MultirotorArmCadParams,
    MultirotorPlateCadParams, TailCadParams, WingCadParams, generate_fuselage_script,
    generate_motor_mount_script, generate_multirotor_arm_script, generate_multirotor_plate_script,
    generate_tail_script, generate_wing_script, py_str_literal, sanitize_filename, sanitize_ident,
};
#[cfg(windows)]
pub use runner::CREATE_NO_WINDOW;
pub use runner::{
    CadExecutionResult, CadLimits, CadRunError, CadRunnerMessage, CancelFlag, FreeCadInstallation,
    Interference, PairMetric, ProcessEnd, STAGE_PREFIX, StageInfo, clear_freecad_cache,
    detect_freecad, detect_freecad_async, detect_freecad_cached, detect_freecad_cached_with_runner,
    detect_freecad_with_runner, find_freecad_gui, get_freecad_version, interpret_cad_output,
    new_command, parse_stage_line, run_freecad_script_async, run_freecad_script_controlled,
    run_freecad_script_sync, run_process_controlled,
};
