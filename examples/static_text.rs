use saftbar::bar::{
    Alignment, Bar, ContentItem, ContentShape, PowerlineDirection, PowerlineFill, PowerlineStyle,
};

fn render(bar: &mut Bar) {
    let red = (255, 0, 0, 255);
    let blue = (0, 0, 255, 255);
    let black = (0, 0, 0, 255);
    let white = (255, 255, 255, 255);
    let green = (0, 255, 0, 255);

    bar.clear_monitors();

    let shape = ContentShape::Powerline(
        PowerlineStyle::Powerline,
        PowerlineFill::Full,
        PowerlineDirection::Right,
    );
    bar.draw(
        0,
        Alignment::Left,
        &[
            ContentItem {
                bg: red,
                fg: black,
                shape: shape.clone(),
            },
            ContentItem {
                bg: red,
                fg: white,
                shape: ContentShape::Text(
                    "t s g g s y j󰌃 p m󰊫 a g         ".to_owned(),
                ),
            },
            ContentItem {
                bg: blue,
                fg: red,
                shape: shape.clone(),
            },
            ContentItem {
                bg: blue,
                fg: black,
                shape: ContentShape::Text("leftlast1".to_owned()),
            },
            ContentItem {
                bg: blue,
                fg: red,
                shape: ContentShape::Powerline(
                    PowerlineStyle::Octagon,
                    PowerlineFill::No,
                    PowerlineDirection::Left,
                ),
            },
            ContentItem {
                bg: blue,
                fg: red,
                shape: ContentShape::Powerline(
                    PowerlineStyle::Octagon,
                    PowerlineFill::No,
                    PowerlineDirection::Right,
                ),
            },
            ContentItem {
                bg: blue,
                fg: red,
                shape: ContentShape::Powerline(
                    PowerlineStyle::Octagon,
                    PowerlineFill::Full,
                    PowerlineDirection::Left,
                ),
            },
            ContentItem {
                bg: black,
                fg: blue,
                shape: ContentShape::Text(" ".to_owned()),
            },
            ContentItem {
                bg: blue,
                fg: red,
                shape: ContentShape::Powerline(
                    PowerlineStyle::Octagon,
                    PowerlineFill::Full,
                    PowerlineDirection::Right,
                ),
            },
            ContentItem {
                bg: blue,
                fg: red,
                shape: ContentShape::Powerline(
                    PowerlineStyle::Powerline,
                    PowerlineFill::No,
                    PowerlineDirection::Left,
                ),
            },
            ContentItem {
                bg: blue,
                fg: red,
                shape: ContentShape::Powerline(
                    PowerlineStyle::Powerline,
                    PowerlineFill::No,
                    PowerlineDirection::Right,
                ),
            },
            ContentItem {
                bg: blue,
                fg: red,
                shape: ContentShape::Powerline(
                    PowerlineStyle::Powerline,
                    PowerlineFill::Full,
                    PowerlineDirection::Left,
                ),
            },
            ContentItem {
                bg: black,
                fg: blue,
                shape: ContentShape::Text(" ".to_owned()),
            },
            ContentItem {
                bg: blue,
                fg: red,
                shape: ContentShape::Powerline(
                    PowerlineStyle::Powerline,
                    PowerlineFill::Full,
                    PowerlineDirection::Right,
                ),
            },
            ContentItem {
                bg: blue,
                fg: black,
                shape: ContentShape::Text("leftlast1a".to_owned()),
            },
            ContentItem {
                bg: black,
                fg: blue,
                shape: shape.clone(),
            },
        ],
    );

    let shape = ContentShape::Powerline(
        PowerlineStyle::Powerline,
        PowerlineFill::Full,
        PowerlineDirection::Left,
    );
    bar.draw(
        0,
        Alignment::Right,
        &[
            ContentItem {
                bg: black,
                fg: green,
                shape: shape.clone(),
            },
            ContentItem {
                bg: green,
                fg: red,
                shape: ContentShape::Text("rightfirst".to_owned()),
            },
            ContentItem {
                bg: green,
                fg: blue,
                shape: ContentShape::Text("rightlast".to_owned()),
            },
            ContentItem {
                bg: green,
                fg: black,
                shape: shape.clone(),
            },
        ],
    );

    let shape = ContentShape::Powerline(
        PowerlineStyle::Octagon,
        PowerlineFill::Full,
        PowerlineDirection::Right,
    );
    bar.draw(
        1,
        Alignment::Left,
        &[
            ContentItem {
                bg: white,
                fg: black,
                shape: shape.clone(),
            },
            ContentItem {
                bg: white,
                fg: black,
                shape: ContentShape::Text(
                    "tsggsyj󰌃pm󰊫agOQIWUOEIRJSLKN<VMCXNV".to_owned(),
                ),
            },
            ContentItem {
                bg: white,
                fg: blue,
                shape: ContentShape::Text("blue".to_owned()),
            },
            ContentItem {
                bg: white,
                fg: green,
                shape: ContentShape::Text("green".to_owned()),
            },
            ContentItem {
                bg: white,
                fg: green,
                shape: ContentShape::Text("green".to_owned()),
            },
            ContentItem {
                bg: white,
                fg: red,
                shape: ContentShape::Text("red".to_owned()),
            },
            ContentItem {
                bg: black,
                fg: white,
                shape: shape.clone(),
            },
        ],
    );

    let shape = ContentShape::Powerline(
        PowerlineStyle::Octagon,
        PowerlineFill::Full,
        PowerlineDirection::Left,
    );
    bar.draw(
        1,
        Alignment::Right,
        &[
            ContentItem {
                bg: black,
                fg: white,
                shape: shape.clone(),
            },
            ContentItem {
                bg: white,
                fg: red,
                shape: ContentShape::Text("          ".to_owned()),
            },
            ContentItem {
                bg: white,
                fg: red,
                shape: shape.clone(),
            },
            ContentItem {
                bg: red,
                fg: white,
                shape: ContentShape::Text("".to_owned()),
            },
            ContentItem {
                bg: red,
                fg: black,
                shape: shape.clone(),
            },
        ],
    );
}

fn main() {
    let mut bar = Bar::new();
    render(&mut bar);
    bar.present();
    bar.flush();
    std::thread::sleep(std::time::Duration::from_secs(10));
}
