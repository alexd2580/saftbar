use saftbar::{
    bar::{Alignment, Bar, ColoredText},
    error::Error,
};

fn render(bar: &mut Bar) -> Result<(), Error> {
    let red = (255, 0, 0, 255);
    let blue = (0, 0, 255, 255);
    let black = (0, 0, 0, 255);
    let white = (255, 255, 255, 255);
    let green = (0, 255, 0, 255);

    bar.clear_monitors()?;
    bar.render_string(
        0,
        Alignment::Left,
        &[
            ColoredText {
                text: "".to_owned(),
                fg: white,
                bg: red,
            },
            ColoredText {
                text: "t s g g s y j p g a g         ".to_owned(),
                fg: red,
                bg: white,
            },
            ColoredText {
                text: "".to_owned(),
                fg: white,
                bg: red,
            },
            ColoredText {
                text: "leftlast1".to_owned(),
                fg: black,
                bg: blue,
            },
        ],
    )?;

    bar.render_string(
        0,
        Alignment::Right,
        &[
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
    )?;

    bar.render_string(
        1,
        Alignment::Left,
        &[
            ColoredText {
                text: "tsggsyjpgagOQIWUOEIRJSLKN<VMCXNV".to_owned(),
                fg: red,
                bg: white,
            },
            ColoredText {
                text: "white black".to_owned(),
                fg: white,
                bg: black,
            },
            ColoredText {
                text: "white red".to_owned(),
                fg: white,
                bg: red,
            },
            ColoredText {
                text: "white blue".to_owned(),
                fg: white,
                bg: blue,
            },
            ColoredText {
                text: "white green".to_owned(),
                fg: white,
                bg: green,
            },
        ],
    )?;

    bar.render_string(
        1,
        Alignment::Right,
        &[
            ColoredText {
                text: "          ".to_owned(),
                fg: white,
                bg: red,
            },
            ColoredText {
                text: "".to_owned(),
                fg: green,
                bg: white,
            },
        ],
    )?;

    Ok(())
}

fn main() {
    let mut bar = Bar::new().unwrap();
    render(&mut bar).unwrap();
    bar.blit().unwrap();
    bar.flush().unwrap();
    std::thread::sleep(std::time::Duration::from_secs(10));
}
