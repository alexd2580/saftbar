use crate::xft::RGBA;

pub struct ColoredText {
    pub text: String,
    pub fg: RGBA,
    pub bg: RGBA,
}
pub struct SingleDisplay {
    pub left: Vec<ColoredText>,
    pub right: Vec<ColoredText>,
}
pub struct InputAnalysis(pub Vec<Option<SingleDisplay>>);

pub fn analyse_string() -> InputAnalysis {
    let red = (65535, 0, 0, 65535);
    let blue = (0, 0, 65535, 65535);
    let black = (0, 0, 0, 65535);
    let white = (65535, 65535, 65535, 65535);
    let green = (0, 65535, 0, 65535);

    InputAnalysis(vec![
        Some(SingleDisplay {
            left: vec![
                ColoredText {
                    text: "leftfirst1".to_owned(),
                    fg: red,
                    bg: white,
                },
                ColoredText {
                    text: "leftlast1".to_owned(),
                    fg: black,
                    bg: blue,
                },
            ],
            right: vec![
                ColoredText {
                    text: "rightfirst1".to_owned(),
                    fg: green,
                    bg: red,
                },
                ColoredText {
                    text: "rightlast1".to_owned(),
                    fg: white,
                    bg: black,
                },
            ],
        }),
        Some(SingleDisplay {
            left: vec![
                ColoredText {
                    text: "leftfirst2".to_owned(),
                    fg: blue,
                    bg: green,
                },
                ColoredText {
                    text: "leftlast2".to_owned(),
                    fg: red,
                    bg: black,
                },
            ],
            right: vec![
                ColoredText {
                    text: "rightfirst2".to_owned(),
                    fg: white,
                    bg: red,
                },
                ColoredText {
                    text: "rightlast2".to_owned(),
                    fg: green,
                    bg: white,
                },
            ],
        }),
    ])
}
