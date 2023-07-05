# Saftbar

I really love reimplementing stuff over and over again. This is the
continuation of the quest for a "nice" allrounder i3 bar which fits my
criteria. I've written some status generators, and got to know the pitfalls.
The biggest pitfall is fonts. Fonts are hard. And for some unknown reason there
is no maintained simple bar that "just" renders nerd-font compatible ttf fonts
into a bar at the top of the screen. Which is why i ~stole~ copied this.

Originally this was [`lemonbar`](https://github.com/LemonBoy/bar) but lemonbar
doesn't have support for xft fonts and subsequently doesn't do well with nerd
fonts. Then there was the xft fork by
[krypt-n](https://github.com/krypt-n/bar), but it is unmodified since 2018 and
archived since 2020, sooo...

# Rewrite

...I decided to rewrite bar in the ~best language - Rust. For fun OFC.

# API

I'm still thinking about how to expose the bar API, but i am leaning towards a
channel based approach for a tight integration with rust clients, removing the
string serialization step inbetween, allowing for potentially more and more
complex features.

# Capabilities

At the moment the bar has less capabilities and is probably less performant,
but instead aims at being more modular and more readable than its
side-effect-ful parents. There's multi-monitor support using randr only,
multicolor, potentially multifont, but without (with minimal) custom geometry
specifications.

# As always

If you find this code to be lacking in any regard, please feel free to open
issues, i'd love to learn more about Rust, X and bar-api design.

# Challenges

Fonts....

![fonts are hard, especially proper powerline
fonts](./images/debug-fonts.png)
