pub const TEMPLATE_COUNT: usize = 5;

pub fn template_name(index: usize) -> &'static str {
    match index % TEMPLATE_COUNT {
        0 => "bipedal",
        1 => "quadruped",
        2 => "blob",
        3 => "winged",
        4 => "serpentine",
        _ => unreachable!(),
    }
}

pub fn template_index(name: &str) -> usize {
    match name {
        "bipedal" => 0,
        "quadruped" => 1,
        "blob" => 2,
        "winged" => 3,
        "serpentine" => 4,
        _ => 0,
    }
}

pub fn get_template(index: usize) -> Vec<Vec<i8>> {
    match index % TEMPLATE_COUNT {
        0 => bipedal(),
        1 => quadruped(),
        2 => blob(),
        3 => winged(),
        4 => serpentine(),
        _ => unreachable!(),
    }
}

fn bipedal() -> Vec<Vec<i8>> {
    vec![
        vec![0,  0,  0,  1,  1,  2],
        vec![0,  0,  1,  1,  2,  2],
        vec![0,  0,  1,  1,  1,  1],
        vec![0,  0,  0,  1,  1,  0],
        vec![0,  0,  1,  1,  1,  1],
        vec![0,  1,  1,  1,  2,  2],
        vec![0,  1,  1,  1,  2,  1],
        vec![0,  1,  1,  1,  2,  2],
        vec![0,  0,  1,  1,  1,  1],
        vec![0,  0,  1,  1,  1,  0],
        vec![0,  0,  1,  1,  0,  0],
        vec![0,  0,  1,  1,  0,  0],
        vec![0,  0,  1,  2,  0,  0],
        vec![0,  0,  1,  2,  0,  0],
    ]
}

fn quadruped() -> Vec<Vec<i8>> {
    vec![
        vec![0,  0,  0,  0,  1,  2],
        vec![0,  0,  1,  1,  2,  2],
        vec![0,  1,  1,  1,  1,  1],
        vec![1,  1,  1,  1,  2,  2],
        vec![1,  1,  1,  2,  2,  1],
        vec![1,  1,  1,  1,  2,  2],
        vec![0,  1,  1,  1,  1,  1],
        vec![0,  1,  0,  0,  1,  0],
        vec![0,  1,  0,  0,  1,  0],
        vec![0,  2,  0,  0,  2,  0],
    ]
}

fn blob() -> Vec<Vec<i8>> {
    vec![
        vec![0,  0,  1,  1,  1],
        vec![0,  1,  2,  2,  2],
        vec![1,  1,  2,  2,  2],
        vec![1,  2,  2,  2,  1],
        vec![1,  2,  2,  2,  2],
        vec![1,  1,  2,  2,  1],
        vec![0,  1,  1,  2,  1],
        vec![0,  0,  1,  1,  0],
    ]
}

fn winged() -> Vec<Vec<i8>> {
    vec![
        vec![0,  0,  0,  0,  1,  1,  0],
        vec![0,  0,  0,  1,  1,  2,  0],
        vec![0,  0,  0,  1,  1,  1,  0],
        vec![0,  0,  0,  0,  1,  0,  0],
        vec![1,  0,  0,  1,  1,  1,  0],
        vec![1,  1,  1,  1,  2,  2,  0],
        vec![0,  1,  1,  1,  1,  2,  0],
        vec![0,  0,  1,  1,  1,  1,  0],
        vec![0,  0,  0,  1,  1,  0,  0],
        vec![0,  0,  0,  1,  0,  0,  0],
        vec![0,  0,  1,  2,  0,  0,  0],
        vec![0,  0,  1,  2,  0,  0,  0],
    ]
}

fn serpentine() -> Vec<Vec<i8>> {
    vec![
        vec![0,  0,  1,  1,  2],
        vec![0,  1,  1,  2,  2],
        vec![0,  1,  1,  1,  1],
        vec![0,  0,  1,  1,  0],
        vec![0,  1,  1,  1,  0],
        vec![0,  1,  2,  1,  0],
        vec![0,  0,  1,  1,  0],
        vec![0,  0,  1,  2,  1],
        vec![0,  1,  1,  2,  1],
        vec![0,  1,  1,  1,  0],
        vec![0,  0,  1,  1,  0],
        vec![0,  0,  1,  2,  0],
        vec![0,  0,  1,  1,  0],
        vec![0,  0,  0,  1,  0],
    ]
}
