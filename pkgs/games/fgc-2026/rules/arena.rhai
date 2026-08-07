// Runtime arena configuration for the FGC 2026 test match.
// This is intentionally data-driven: the Rust runtime reads this function
// and does not hard-code the object count, shape, or motion parameters.
fn arena_config() {
    #{
        physics_backend: "sphere_xpbd",
        solver: #{
            // The sphere-specialized solver keeps dense ball contacts stable
            // without paying for a general rigid-body constraint graph.
            position_iterations: 3.0,
            velocity_iterations: 4.0,
            contact_compliance: 0.0000002,
            max_depenetration_speed_mps: 2.5,
            max_ball_speed_mps: 12.0,
            max_ball_angular_speed_radps: 240.0,
            sleep_linear_threshold_mps: 0.035,
            sleep_angular_threshold_radps: 0.20,
            sleep_after_seconds: 0.75,
            restitution_velocity_threshold_mps: 0.20
        },
        object_id: "wildfire",
        // The real match release is 500 Wildfire balls. Stress tests may
        // override this at runtime without changing the competition pack.
        object_count: 500,
        // Balls originate at the semantic EXTballspawn dispenser; they are
        // released in a deterministic four-second pour, not scattered.
        spawn_radius: 0.045,
        spawn_height: 1.8,
        // TEMP: EXTballspawn was authored too high in Blender. Remove this
        // offset after correcting the semantic node in the field asset.
        spawn_offset_y_m: -1.55,
        spawn_release_seconds: 4.0,
        spawn_fountain_vertical_speed_mps: 0.14,
        spawn_fountain_forward_speed_mps: 2.55,
        spawn_fountain_spread_mps: 0.72,
        gravity_scale: 1.0,
        ball_to_ball_collisions: true,
        color: "#f97316",

        // Official game-piece measurements. The contact coefficients are kept
        // here so they can be calibrated without rebuilding the game server.
        ball: #{
            material: "closed-cell polyurethane foam",
            diameter_m: 0.100,
            diameter_tolerance_m: 0.003,
            mass_kg: 0.062,
            friction: 0.55,
            restitution: 0.32,
            // Air drag is small. Carpet losses are modeled separately so the
            // ball does not slow down unrealistically while airborne.
            linear_damping: 0.02,
            angular_damping: 0.08,
            rolling_resistance_mps2: 0.40,
            // Predict roughly one diameter ahead without the full cost of CCD.
            soft_ccd_prediction_m: 0.12,
            inertia_factor: 0.40,
            drag_coefficient: 0.47,
            air_density_kg_m3: 1.225,
            // Dense dispenser flow is governed primarily by ball↔ball loss.
            // Keep enough slip damping to avoid billiard-like motion, but do
            // not let each foam contact absorb the whole forward stream.
            ball_friction: 0.18,
            restitution_curve: #{
                low_speed: 0.48,
                high_speed: 0.32,
                transition_speed_mps: 1.50,
                exponent: 0.65
            }
        },

        floor: #{
            material: "low-pile carpet",
            friction: 0.85,
            restitution: 0.05,
            static_friction: 0.85,
            dynamic_friction: 0.65,
            rolling_resistance_mps2: 0.55,
            restitution_curve: #{
                low_speed: 0.18,
                high_speed: 0.06,
                transition_speed_mps: 1.20,
                exponent: 0.70
            }
        },

        // Authoritative 50 cm cubic, 18 kg robot envelope. These values model
        // finite motor force, wheel scrub, braking and steering acceleration.
        robot: #{
            mass_kg: 18.0,
            width_m: 0.50,
            height_m: 0.50,
            length_m: 0.50,
            track_width_m: 0.42,
            traction_friction: 1.05,
            surface_friction: 0.45,
            restitution: 0.05,
            rolling_resistance: 0.35,
            max_speed_mps: 4.0,
            max_acceleration_mps2: 3.5,
            max_deceleration_mps2: 5.0,
            // Force and power limits keep the controller from behaving like
            // an ideal velocity source when the chassis is heavily loaded.
            max_drive_force_n: 140.0,
            max_drive_power_w: 420.0,
            max_brake_force_n: 200.0,
            max_turn_rate_radps: 4.5,
            max_angular_acceleration_radps2: 9.0,
            lateral_grip_mps2: 12.0,
            restitution_curve: #{
                low_speed: 0.36,
                high_speed: 0.16,
                transition_speed_mps: 1.80,
                exponent: 0.65
            },
            intake_enabled: true,
            intake_width_m: 0.52,
            intake_radius_m: 0.045,
            intake_forward_offset_m: 0.38,
            intake_center_height_m: 0.075,
            intake_surface_speed_mps: 3.0,
            intake_friction: 0.90,
            intake_normal_force_n: 8.0,
            intake_restitution_curve: #{
                low_speed: 0.22,
                high_speed: 0.10,
                transition_speed_mps: 1.50,
                exponent: 0.70
            },
            // Hopper storage + wide flywheel outtake. The flywheel spans 3.5
            // WILDFIRE widths so the mouth accepts a spread of incoming balls
            // and launches them across a matching opening.
            storage_capacity: 40.0,
            intake_rate_bps: 6.0,
            outtake_rate_bps: 3.0,
            outtake_velocity_mps: 8.0,
            outtake_angle_deg: 35.0,
            flywheel_width_m: 0.35,
            outtake_forward_offset_m: 0.0,
            // Above the 50 cm chassis top so a launched ball clears the body.
            outtake_height_m: 0.55
        },

        goal_wall: #{
            material: "polycarbonate",
            static_friction: 0.35,
            dynamic_friction: 0.28,
            restitution_curve: #{
                low_speed: 0.38,
                high_speed: 0.18,
                transition_speed_mps: 1.80,
                exponent: 0.65
            }
        },

        metal_wall: #{
            material: "powder-coated steel/aluminum",
            static_friction: 0.45,
            dynamic_friction: 0.32,
            restitution_curve: #{
                low_speed: 0.42,
                high_speed: 0.20,
                transition_speed_mps: 2.00,
                exponent: 0.65
            }
        },

        // Calibration fixture representing an inclined containment surface.
        ramp: #{
            enabled: true,
            center_x: -3.0,
            start_z: -1.0,
            width_m: 2.0,
            length_m: 2.0,
            angle_deg: 15.0,
            material: "polycarbonate ramp",
            static_friction: 0.40,
            dynamic_friction: 0.30,
            restitution_curve: #{
                low_speed: 0.25,
                high_speed: 0.09,
                transition_speed_mps: 1.30,
                exponent: 0.70
            }
        }
    }
}
