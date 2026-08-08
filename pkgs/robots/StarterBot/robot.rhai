// Starter Robot control contract.
// The host applies this to every input before it reaches the authoritative
// drivetrain and mechanism simulation.

fn limit(value, low, high) {
    if value < low { return low; }
    if value > high { return high; }
    value
}

fn robot_input(input) {
    let move_x = limit(input.move_x, -1.0, 1.0);
    let move_z = limit(input.move_z, -1.0, 1.0);
    #{
        move_x: move_x,
        move_z: move_z,
        intake_power: 0.0,
        outtake_power: 0.0
    }
}
