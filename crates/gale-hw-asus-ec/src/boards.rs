// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Board table and register maps generated from LibreHardwareMonitor,
// LibreHardwareMonitorLib/Hardware/Motherboard/Lpc/EC/EmbeddedController.cs and
// Hardware/Motherboard/Identification.cs. Only temperature and fan sources are kept.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Temperature,
    Fan,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Source {
    pub slug: &'static str,
    pub name: &'static str,
    pub kind: Kind,
    pub register: u16,
    pub size: u8,
    pub factor: f64,
    pub offset: f64,
    pub blank: Option<i32>,
    pub little_endian: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Amd400,
    Amd500,
    Amd600,
    Amd800,
    Intel100,
    Intel300,
    Intel370,
    Intel400,
    Intel600,
    Intel700,
}

pub struct Board {
    pub product: &'static str,
    pub family: Family,
    pub sensors: &'static [&'static str],
}

const AMD400_SOURCES: &[(&str, Source)] = &[
    (
        "TempChipset",
        Source {
            slug: "chipset",
            name: "Chipset",
            kind: Kind::Temperature,
            register: 0x003a,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "TempCPU",
        Source {
            slug: "cpu",
            name: "CPU",
            kind: Kind::Temperature,
            register: 0x003b,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "TempMB",
        Source {
            slug: "motherboard",
            name: "Motherboard",
            kind: Kind::Temperature,
            register: 0x003c,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "TempTSensor",
        Source {
            slug: "t_sensor",
            name: "T Sensor",
            kind: Kind::Temperature,
            register: 0x003d,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
    (
        "TempVrm",
        Source {
            slug: "vrm",
            name: "VRM",
            kind: Kind::Temperature,
            register: 0x003e,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "FanCPUOpt",
        Source {
            slug: "cpu_optional_fan",
            name: "CPU Optional Fan",
            kind: Kind::Fan,
            register: 0x00bc,
            size: 2,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "FanVrmHS",
        Source {
            slug: "vrm_heat_sink_fan",
            name: "VRM Heat Sink Fan",
            kind: Kind::Fan,
            register: 0x00b2,
            size: 2,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "TempWaterIn",
        Source {
            slug: "water_in",
            name: "Water In",
            kind: Kind::Temperature,
            register: 0x010d,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
    (
        "TempWaterOut",
        Source {
            slug: "water_out",
            name: "Water Out",
            kind: Kind::Temperature,
            register: 0x010b,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
];

const AMD500_SOURCES: &[(&str, Source)] = &[
    (
        "TempChipset",
        Source {
            slug: "chipset",
            name: "Chipset",
            kind: Kind::Temperature,
            register: 0x003a,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "TempCPU",
        Source {
            slug: "cpu",
            name: "CPU",
            kind: Kind::Temperature,
            register: 0x003b,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "TempMB",
        Source {
            slug: "motherboard",
            name: "Motherboard",
            kind: Kind::Temperature,
            register: 0x003c,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "TempTSensor",
        Source {
            slug: "t_sensor",
            name: "T Sensor",
            kind: Kind::Temperature,
            register: 0x003d,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
    (
        "TempVrm",
        Source {
            slug: "vrm",
            name: "VRM",
            kind: Kind::Temperature,
            register: 0x003e,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "FanCPUOpt",
        Source {
            slug: "cpu_optional_fan",
            name: "CPU Optional Fan",
            kind: Kind::Fan,
            register: 0x00b0,
            size: 2,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "FanVrmHS",
        Source {
            slug: "vrm_heat_sink_fan",
            name: "VRM Heat Sink Fan",
            kind: Kind::Fan,
            register: 0x00b2,
            size: 2,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "FanChipset",
        Source {
            slug: "chipset_fan",
            name: "Chipset Fan",
            kind: Kind::Fan,
            register: 0x00b4,
            size: 2,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "TempWaterIn",
        Source {
            slug: "water_in",
            name: "Water In",
            kind: Kind::Temperature,
            register: 0x0100,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
    (
        "TempWaterOut",
        Source {
            slug: "water_out",
            name: "Water Out",
            kind: Kind::Temperature,
            register: 0x0101,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
];

const AMD600_SOURCES: &[(&str, Source)] = &[
    (
        "FanCPUOpt",
        Source {
            slug: "cpu_optional_fan",
            name: "CPU Optional Fan",
            kind: Kind::Fan,
            register: 0x00b0,
            size: 2,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "TempWaterIn",
        Source {
            slug: "water_in",
            name: "Water In",
            kind: Kind::Temperature,
            register: 0x0100,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
    (
        "TempWaterOut",
        Source {
            slug: "water_out",
            name: "Water Out",
            kind: Kind::Temperature,
            register: 0x0101,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
];

const AMD800_SOURCES: &[(&str, Source)] = &[
    (
        "TempCPU",
        Source {
            slug: "cpu",
            name: "CPU",
            kind: Kind::Temperature,
            register: 0x0030,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "TempCPUPackage",
        Source {
            slug: "cpu_package",
            name: "CPU Package",
            kind: Kind::Temperature,
            register: 0x0031,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "TempMB",
        Source {
            slug: "motherboard",
            name: "Motherboard",
            kind: Kind::Temperature,
            register: 0x0032,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "TempVrm",
        Source {
            slug: "vrm",
            name: "VRM",
            kind: Kind::Temperature,
            register: 0x0033,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "TempTSensor",
        Source {
            slug: "t_sensor",
            name: "T Sensor",
            kind: Kind::Temperature,
            register: 0x0036,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
    (
        "TempTSensorAlt",
        Source {
            slug: "t_sensor",
            name: "T Sensor",
            kind: Kind::Temperature,
            register: 0x0035,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
    (
        "FanCPUOpt",
        Source {
            slug: "cpu_optional_fan",
            name: "CPU Optional Fan",
            kind: Kind::Fan,
            register: 0x00b0,
            size: 2,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
];

const INTEL100_SOURCES: &[(&str, Source)] = &[
    (
        "TempChipset",
        Source {
            slug: "chipset",
            name: "Chipset",
            kind: Kind::Temperature,
            register: 0x003a,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "TempVrm",
        Source {
            slug: "vrm",
            name: "VRM",
            kind: Kind::Temperature,
            register: 0x003e,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "TempTSensor",
        Source {
            slug: "t_sensor",
            name: "T Sensor",
            kind: Kind::Temperature,
            register: 0x003d,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
    (
        "FanWaterPump",
        Source {
            slug: "water_pump",
            name: "Water Pump",
            kind: Kind::Fan,
            register: 0x00bc,
            size: 2,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
];

const INTEL300_SOURCES: &[(&str, Source)] = &[
    (
        "TempVrm",
        Source {
            slug: "vrm",
            name: "VRM",
            kind: Kind::Temperature,
            register: 0x003e,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "TempChipset",
        Source {
            slug: "chipset",
            name: "Chipset",
            kind: Kind::Temperature,
            register: 0x003a,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "TempTSensor",
        Source {
            slug: "t_sensor",
            name: "T Sensor",
            kind: Kind::Temperature,
            register: 0x003d,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
    (
        "TempWaterIn",
        Source {
            slug: "water_in",
            name: "Water In",
            kind: Kind::Temperature,
            register: 0x0100,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
    (
        "TempWaterOut",
        Source {
            slug: "water_out",
            name: "Water Out",
            kind: Kind::Temperature,
            register: 0x0101,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
    (
        "FanCPUOpt",
        Source {
            slug: "cpu_optional_fan",
            name: "CPU Optional Fan",
            kind: Kind::Fan,
            register: 0x00b0,
            size: 2,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
];

const INTEL370_SOURCES: &[(&str, Source)] = &[
    (
        "TempChipset",
        Source {
            slug: "chipset",
            name: "Chipset",
            kind: Kind::Temperature,
            register: 0x003a,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "TempTSensor",
        Source {
            slug: "t_sensor",
            name: "T Sensor",
            kind: Kind::Temperature,
            register: 0x003d,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
    (
        "FanCPUOpt",
        Source {
            slug: "cpu_optional_fan",
            name: "CPU Optional Fan",
            kind: Kind::Fan,
            register: 0x00bc,
            size: 2,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "FanWaterPump",
        Source {
            slug: "water_pump",
            name: "Water Pump",
            kind: Kind::Fan,
            register: 0x00be,
            size: 2,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
];

const INTEL400_SOURCES: &[(&str, Source)] = &[
    (
        "TempChipset",
        Source {
            slug: "chipset",
            name: "Chipset",
            kind: Kind::Temperature,
            register: 0x003a,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "TempTSensor",
        Source {
            slug: "t_sensor",
            name: "T Sensor",
            kind: Kind::Temperature,
            register: 0x003d,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
    (
        "TempVrm",
        Source {
            slug: "vrm",
            name: "VRM",
            kind: Kind::Temperature,
            register: 0x003e,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "FanCPUOpt",
        Source {
            slug: "cpu_optional_fan",
            name: "CPU Optional Fan",
            kind: Kind::Fan,
            register: 0x00b0,
            size: 2,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "TempWaterIn",
        Source {
            slug: "water_in",
            name: "Water In",
            kind: Kind::Temperature,
            register: 0x0100,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
    (
        "TempWaterOut",
        Source {
            slug: "water_out",
            name: "Water Out",
            kind: Kind::Temperature,
            register: 0x0101,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
];

const INTEL600_SOURCES: &[(&str, Source)] = &[
    (
        "TempTSensor",
        Source {
            slug: "t_sensor",
            name: "T Sensor",
            kind: Kind::Temperature,
            register: 0x003d,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
    (
        "TempVrm",
        Source {
            slug: "vrm",
            name: "VRM",
            kind: Kind::Temperature,
            register: 0x003e,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "TempWaterIn",
        Source {
            slug: "water_in",
            name: "Water In",
            kind: Kind::Temperature,
            register: 0x0100,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
    (
        "TempWaterOut",
        Source {
            slug: "water_out",
            name: "Water Out",
            kind: Kind::Temperature,
            register: 0x0101,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
    (
        "TempWaterBlockIn",
        Source {
            slug: "water_block_in",
            name: "Water Block In",
            kind: Kind::Temperature,
            register: 0x0102,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
];

const INTEL700_SOURCES: &[(&str, Source)] = &[
    (
        "TempVrm",
        Source {
            slug: "vrm",
            name: "VRM",
            kind: Kind::Temperature,
            register: 0x0033,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "FanCPUOpt",
        Source {
            slug: "cpu_optional_fan",
            name: "CPU Optional Fan",
            kind: Kind::Fan,
            register: 0x00b0,
            size: 2,
            factor: 1.0,
            offset: 0.0,
            blank: None,
            little_endian: false,
        },
    ),
    (
        "TempTSensor",
        Source {
            slug: "t_sensor",
            name: "T Sensor",
            kind: Kind::Temperature,
            register: 0x0109,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
    (
        "TempTSensor2",
        Source {
            slug: "t_sensor_2",
            name: "T Sensor 2",
            kind: Kind::Temperature,
            register: 0x0105,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
    (
        "TempWaterIn",
        Source {
            slug: "water_in",
            name: "Water In",
            kind: Kind::Temperature,
            register: 0x0100,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
    (
        "TempWaterOut",
        Source {
            slug: "water_out",
            name: "Water Out",
            kind: Kind::Temperature,
            register: 0x0101,
            size: 1,
            factor: 1.0,
            offset: 0.0,
            blank: Some(-40),
            little_endian: false,
        },
    ),
];

pub fn family_sources(family: Family) -> &'static [(&'static str, Source)] {
    match family {
        Family::Amd400 => AMD400_SOURCES,
        Family::Amd500 => AMD500_SOURCES,
        Family::Amd600 => AMD600_SOURCES,
        Family::Amd800 => AMD800_SOURCES,
        Family::Intel100 => INTEL100_SOURCES,
        Family::Intel300 => INTEL300_SOURCES,
        Family::Intel370 => INTEL370_SOURCES,
        Family::Intel400 => INTEL400_SOURCES,
        Family::Intel600 => INTEL600_SOURCES,
        Family::Intel700 => INTEL700_SOURCES,
    }
}

pub const BOARDS: &[Board] = &[
    Board {
        product: "ROG STRIX B850-E GAMING WIFI",
        family: Family::Amd800,
        sensors: &["TempCPUPackage", "TempVrm", "TempTSensorAlt"],
    },
    Board {
        product: "ROG CROSSHAIR X870E DARK HERO",
        family: Family::Amd800,
        sensors: &["TempTSensor"],
    },
    Board {
        product: "ROG CROSSHAIR X870E HERO BTF",
        family: Family::Amd800,
        sensors: &[
            "TempCPU",
            "TempCPUPackage",
            "TempMB",
            "TempVrm",
            "TempTSensor",
            "FanCPUOpt",
        ],
    },
    Board {
        product: "TUF GAMING X870-PLUS WIFI",
        family: Family::Amd800,
        sensors: &["TempVrm", "FanCPUOpt"],
    },
    Board {
        product: "ROG STRIX X870E-E GAMING WIFI",
        family: Family::Amd800,
        sensors: &[
            "TempCPU",
            "TempCPUPackage",
            "TempMB",
            "TempVrm",
            "FanCPUOpt",
        ],
    },
    Board {
        product: "PRIME X470-PRO",
        family: Family::Amd400,
        sensors: &[
            "TempChipset",
            "TempCPU",
            "TempMB",
            "TempVrm",
            "FanCPUOpt",
            "CurrCPU",
            "VoltageCPU",
        ],
    },
    Board {
        product: "PRIME X570-PRO",
        family: Family::Amd500,
        sensors: &[
            "TempChipset",
            "TempCPU",
            "TempMB",
            "TempVrm",
            "TempTSensor",
            "FanChipset",
        ],
    },
    Board {
        product: "ProArt X570-CREATOR WIFI",
        family: Family::Amd500,
        sensors: &[
            "TempChipset",
            "TempCPU",
            "TempMB",
            "TempVrm",
            "TempTSensor",
            "FanCPUOpt",
            "CurrCPU",
            "VoltageCPU",
        ],
    },
    Board {
        product: "Pro WS X570-ACE",
        family: Family::Amd500,
        sensors: &[
            "TempChipset",
            "TempCPU",
            "TempMB",
            "TempVrm",
            "FanChipset",
            "CurrCPU",
            "VoltageCPU",
        ],
    },
    Board {
        product: "ROG CROSSHAIR X670E EXTREME",
        family: Family::Amd600,
        sensors: &["TempWaterIn", "TempWaterOut", "FanCPUOpt"],
    },
    Board {
        product: "ROG CROSSHAIR X670E HERO",
        family: Family::Amd600,
        sensors: &["TempWaterIn", "TempWaterOut", "FanCPUOpt"],
    },
    Board {
        product: "ROG CROSSHAIR X670E GENE",
        family: Family::Amd600,
        sensors: &["TempWaterIn", "TempWaterOut", "FanCPUOpt"],
    },
    Board {
        product: "ROG STRIX X670E-E GAMING WIFI",
        family: Family::Amd600,
        sensors: &["TempWaterIn", "TempWaterOut", "FanCPUOpt"],
    },
    Board {
        product: "ROG STRIX X670E-F GAMING WIFI",
        family: Family::Amd600,
        sensors: &["TempWaterIn", "TempWaterOut", "FanCPUOpt"],
    },
    Board {
        product: "ROG CROSSHAIR VIII DARK HERO",
        family: Family::Amd500,
        sensors: &[
            "TempChipset",
            "TempCPU",
            "TempMB",
            "TempTSensor",
            "TempVrm",
            "TempWaterIn",
            "TempWaterOut",
            "FanCPUOpt",
            "FanWaterFlow",
            "CurrCPU",
            "VoltageCPU",
        ],
    },
    Board {
        product: "ROG CROSSHAIR VIII IMPACT",
        family: Family::Amd500,
        sensors: &[
            "TempChipset",
            "TempCPU",
            "TempMB",
            "TempTSensor",
            "TempVrm",
            "FanChipset",
            "CurrCPU",
            "VoltageCPU",
        ],
    },
    Board {
        product: "ROG STRIX B550-E GAMING",
        family: Family::Amd500,
        sensors: &[
            "TempChipset",
            "TempCPU",
            "TempMB",
            "TempTSensor",
            "TempVrm",
            "FanCPUOpt",
        ],
    },
    Board {
        product: "ROG STRIX B550-I GAMING",
        family: Family::Amd500,
        sensors: &[
            "TempChipset",
            "TempCPU",
            "TempMB",
            "TempTSensor",
            "TempVrm",
            "FanVrmHS",
            "CurrCPU",
            "VoltageCPU",
        ],
    },
    Board {
        product: "ROG STRIX X570-E GAMING",
        family: Family::Amd500,
        sensors: &[
            "TempChipset",
            "TempCPU",
            "TempMB",
            "TempTSensor",
            "TempVrm",
            "FanChipset",
            "CurrCPU",
            "VoltageCPU",
        ],
    },
    Board {
        product: "ROG STRIX X570-E GAMING WIFI II",
        family: Family::Amd500,
        sensors: &["TempChipset", "TempCPU", "TempMB", "TempTSensor", "TempVrm"],
    },
    Board {
        product: "ROG STRIX X570-F GAMING",
        family: Family::Amd500,
        sensors: &[
            "TempChipset",
            "TempCPU",
            "TempMB",
            "TempTSensor",
            "FanChipset",
        ],
    },
    Board {
        product: "ROG STRIX X570-I GAMING",
        family: Family::Amd500,
        sensors: &[
            "TempTSensor",
            "FanVrmHS",
            "FanChipset",
            "CurrCPU",
            "VoltageCPU",
            "TempChipset",
            "TempVrm",
        ],
    },
    Board {
        product: "ROG STRIX Z370-G GAMING",
        family: Family::Intel370,
        sensors: &["TempChipset", "TempTSensor", "FanCPUOpt", "FanWaterPump"],
    },
    Board {
        product: "ROG STRIX Z390-E GAMING",
        family: Family::Intel300,
        sensors: &["TempVrm", "TempChipset", "TempTSensor", "FanCPUOpt"],
    },
    Board {
        product: "ROG STRIX Z390-F GAMING",
        family: Family::Intel300,
        sensors: &["TempVrm", "TempChipset", "TempTSensor", "FanCPUOpt"],
    },
    Board {
        product: "ROG STRIX Z390-I GAMING",
        family: Family::Intel300,
        sensors: &["TempVrm", "TempChipset", "TempTSensor"],
    },
    Board {
        product: "ROG MAXIMUS XI FORMULA",
        family: Family::Intel300,
        sensors: &[
            "TempVrm",
            "TempChipset",
            "TempWaterIn",
            "TempWaterOut",
            "TempTSensor",
            "FanCPUOpt",
        ],
    },
    Board {
        product: "ROG MAXIMUS XII FORMULA",
        family: Family::Intel400,
        sensors: &[
            "TempTSensor",
            "TempVrm",
            "TempWaterIn",
            "TempWaterOut",
            "FanWaterFlow",
        ],
    },
    Board {
        product: "ROG STRIX Z690-A GAMING WIFI D4",
        family: Family::Intel600,
        sensors: &["TempTSensor", "TempVrm"],
    },
    Board {
        product: "ROG STRIX Z690-G GAMING WIFI",
        family: Family::Intel600,
        sensors: &["TempTSensor", "TempVrm"],
    },
    Board {
        product: "ROG MAXIMUS Z690 HERO",
        family: Family::Intel600,
        sensors: &["TempTSensor", "TempWaterIn", "TempWaterOut", "FanWaterFlow"],
    },
    Board {
        product: "ROG MAXIMUS Z690 FORMULA",
        family: Family::Intel600,
        sensors: &[
            "TempTSensor",
            "TempVrm",
            "TempWaterIn",
            "TempWaterOut",
            "TempWaterBlockIn",
            "FanWaterFlow",
        ],
    },
    Board {
        product: "ROG MAXIMUS Z690 EXTREME GLACIAL",
        family: Family::Intel600,
        sensors: &[
            "TempVrm",
            "TempWaterIn",
            "TempWaterOut",
            "TempWaterBlockIn",
            "FanWaterFlow",
        ],
    },
    Board {
        product: "ROG MAXIMUS Z790 HERO",
        family: Family::Intel700,
        sensors: &[
            "TempVrm",
            "TempTSensor",
            "TempWaterIn",
            "TempWaterOut",
            "FanWaterFlow",
            "FanCPUOpt",
        ],
    },
    Board {
        product: "ROG MAXIMUS Z790 DARK HERO",
        family: Family::Intel700,
        sensors: &[
            "TempVrm",
            "FanCPUOpt",
            "TempTSensor",
            "TempWaterIn",
            "TempWaterOut",
            "FanWaterFlow",
        ],
    },
    Board {
        product: "Z170-A",
        family: Family::Intel100,
        sensors: &[
            "TempTSensor",
            "TempChipset",
            "FanWaterPump",
            "CurrCPU",
            "VoltageCPU",
        ],
    },
    Board {
        product: "Z170 PRO GAMING",
        family: Family::Intel100,
        sensors: &["TempChipset", "TempVrm", "TempTSensor"],
    },
    Board {
        product: "PRIME Z690-A",
        family: Family::Intel600,
        sensors: &["TempTSensor", "TempVrm"],
    },
    Board {
        product: "ROG STRIX Z790-I GAMING WIFI",
        family: Family::Intel700,
        sensors: &["TempTSensor", "TempTSensor2"],
    },
    Board {
        product: "ROG STRIX Z790-E GAMING WIFI",
        family: Family::Intel700,
        sensors: &["TempWaterIn"],
    },
    Board {
        product: "ROG STRIX Z790-E GAMING WIFI II",
        family: Family::Intel700,
        sensors: &["TempTSensor", "TempVrm", "FanCPUOpt"],
    },
    Board {
        product: "ROG MAXIMUS Z790 FORMULA",
        family: Family::Intel700,
        sensors: &["TempWaterIn", "TempWaterOut"],
    },
    Board {
        product: "ROG MAXIMUS XII HERO (WI-FI)",
        family: Family::Intel400,
        sensors: &[
            "TempTSensor",
            "TempChipset",
            "TempVrm",
            "TempWaterIn",
            "TempWaterOut",
            "CurrCPU",
            "FanCPUOpt",
            "FanWaterFlow",
        ],
    },
    Board {
        product: "ROG STRIX X870-I GAMING WIFI",
        family: Family::Amd800,
        sensors: &["TempCPU", "TempCPUPackage", "TempMB", "TempVrm"],
    },
];

pub fn find_board(product: &str) -> Option<&'static Board> {
    let wanted = product.trim();
    BOARDS
        .iter()
        .find(|board| board.product.eq_ignore_ascii_case(wanted))
}

pub fn board_sources(board: &Board) -> Vec<Source> {
    let table = family_sources(board.family);
    board
        .sensors
        .iter()
        .filter_map(|sensor| {
            table
                .iter()
                .find(|(name, _)| name == sensor)
                .map(|(_, source)| *source)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prime_x570_pro_maps_to_the_amd500_chipset_sensors() {
        let board = find_board("PRIME X570-PRO").unwrap();
        assert_eq!(board.family, Family::Amd500);
        let sources = board_sources(board);
        let slugs: Vec<&str> = sources.iter().map(|s| s.slug).collect();
        assert_eq!(
            slugs,
            vec![
                "chipset",
                "cpu",
                "motherboard",
                "vrm",
                "t_sensor",
                "chipset_fan"
            ]
        );
        let fan = sources.iter().find(|s| s.slug == "chipset_fan").unwrap();
        assert_eq!((fan.kind, fan.register, fan.size), (Kind::Fan, 0x00b4, 2));
        let t_sensor = sources.iter().find(|s| s.slug == "t_sensor").unwrap();
        assert_eq!(t_sensor.blank, Some(-40));
    }

    #[test]
    fn lookup_is_case_insensitive_and_trims() {
        assert!(find_board("  prime x570-pro ").is_some());
        assert!(find_board("Some Other Board").is_none());
    }

    #[test]
    fn every_board_sensor_exists_in_its_family_or_is_a_dropped_kind() {
        for board in BOARDS {
            for sensor in board.sensors {
                let known = family_sources(board.family)
                    .iter()
                    .any(|(name, _)| name == sensor);
                let dropped = matches!(*sensor, "VoltageCPU" | "CurrCPU" | "FanWaterFlow");
                assert!(known || dropped, "{} {}", board.product, sensor);
            }
        }
    }
}
