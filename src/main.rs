use {
    crate::csv::CovidInstance, clap::Parser, dioxus::prelude::*, fukuoka_c19::csv,
    std::collections::HashMap,
};

#[derive(Clone, Debug, Parser)]
#[clap(author, version, about)]
struct Config {
    /// Just download the csv w/o GUI
    #[clap(long = "headless")]
    headless: bool,
}

#[tokio::main]
async fn main() {
    let config = Config::parse();
    if config.headless {
        match csv::load_csv().await {
            Ok(csv) if csv.is_empty() => {
                println!("The data was downloaded. But it's empty.");
            }
            Ok(csv) => {
                println!("{csv:?}");
            }
            _ => {
                println!("Failed to download.");
            }
        }
    } else {
        dioxus_desktop::launch(App);
        // dioxus_desktop::launch_cfg(App, |cfg| cfg.with_window(|w| w.with_title("Fukuoka C19")));
    }
}

#[derive(Copy, Clone, PartialEq)]
enum TableMode {
    Date,
    Location,
    Age,
}

#[allow(non_snake_case)]
fn App(cx: Scope) -> Element {
    let csv = use_future(cx, (), |_| async move { csv::load_csv().await });
    let display_mode = use_state(cx, || TableMode::Date);
    match csv.value() {
        Some(Ok(csv)) if csv.is_empty() => cx.render(rsx!(
            div {
                    "The downloaded data is empty."
                }
        )),
        Some(Ok(csv)) => {
            let date = &csv.last().unwrap().date;
            let len = csv.len();
            let (ages, dates, locs) = build_tables(csv);
            let (table_data, with_ema) = match *display_mode.current() {
                TableMode::Age => (ages, false),
                TableMode::Date => (dates, true),
                TableMode::Location => (locs, false),
            };
            let render_button = |mode: TableMode, label: &str| {
                let class = if *display_mode == mode {
                    "current-mode"
                } else {
                    "other-mode"
                };
                cx.render(rsx!(
                    button {
                        onclick: move |_| { display_mode.modify(|_| mode)},
                        class: "{class}",
                        "{label}"
                    }
                ))
            };
            let button_age = render_button(TableMode::Age, "世代別");
            let button_date = render_button(TableMode::Date, "時間順");
            let button_loc = render_button(TableMode::Location, "地区別");
            cx.render(rsx!(
                h1 {
                    // style: [include_str!("../assets/main.scss")].into(),
                    style: "font-family: sans-serif; font-size: 20px;",
                    "福岡県COVID-19新規感染者{len}人(2022/04/01 -- {date})"
                }
                button_age
                button_date
                button_loc
                Table {
                    data: table_data,
                    with_ema: with_ema,
                }
            ))
        }
        _ => cx.render(rsx!("Fetching data ...")),
    }
}

#[allow(clippy::type_complexity)]
fn build_tables(csv: &[CovidInstance]) -> (Vec<(&str, u32)>, Vec<(&str, u32)>, Vec<(&str, u32)>) {
    let mut ht_locs: HashMap<&str, u32> = HashMap::new();
    let mut ht_dates: HashMap<&str, u32> = HashMap::new();
    let mut ht_ages: HashMap<&str, u32> = HashMap::new();
    for ci in csv.iter() {
        *ht_dates.entry(&ci.date).or_insert(0) += 1;
        *ht_locs.entry(&ci.location).or_insert(0) += 1;
        *ht_ages.entry(&ci.age).or_insert(0) += 1;
    }
    let mut ages: Vec<(&str, u32)> = ht_ages
        .iter()
        .map(|(k, v)| (*k, *v))
        .filter(|(k, _)| !k.is_empty())
        .collect();
    ages.sort_by_cached_key(|(e, _)| {
        if *e == "10歳未満" {
            return "10代".to_string();
        }
        let mut s = e.to_string();
        if s.chars().count() == 3 {
            s.push('_');
        }
        s
    });
    // ages.sort_unstable();
    let mut dates: Vec<(&str, u32)> = ht_dates.iter().map(|(k, v)| (*k, *v)).collect();
    dates.sort_unstable();
    dates = dates
        .iter()
        .skip(dates.len().saturating_sub(50))
        .copied()
        .collect::<Vec<_>>();
    let mut locs: Vec<(&str, u32)> = ht_locs
        .iter()
        .map(|(k, v)| (*k, *v))
        .filter(|(k, v)| !k.is_empty() && 100 <= *v)
        .collect();
    locs.sort_by_cached_key(|i| -(i.1 as i32));
    (ages, dates, locs)
}

#[derive(Default, PartialEq, PartialOrd, Props)]
struct TableProps<'a> {
    data: Vec<(&'a str, u32)>,
    with_ema: bool,
}

#[allow(non_snake_case)]
fn Table<'a>(cx: Scope<'a, TableProps<'a>>) -> Element {
    if cx.props.data.is_empty() {
        return cx.render(rsx!(
            div { "no data" }
        ));
    }
    let graph_width: f32 = 400.0;
    let graph_height: f32 = 100.0;
    let height: f32 = cx
        .props
        .data
        .iter()
        .map(|e| (e.1 / 2000 + 1) * 2000)
        .max()
        .unwrap() as f32;
    let width: f32 = cx.props.data.len() as f32;
    let scale_w = graph_width / (width - 1.0);
    let scale_h = graph_height / height;
    let path = format!(
        "M0,{:.2} {}",
        graph_height - (cx.props.data[0].1 as f32) * scale_h,
        cx.props
            .data
            .iter()
            .enumerate()
            .map(|(i, (_, v))| {
                format!(
                    "L{:.2},{:.2}",
                    i as f32 * scale_w,
                    graph_height - *v as f32 * scale_h
                )
            })
            .collect::<Vec<_>>()
            .join(" "),
    );
    let mut value_vec: Vec<f32> = cx
        .props
        .data
        .iter()
        .map(|(_, v)| *v as f32)
        .clone()
        .collect::<Vec<_>>();
    let line_ema = if cx.props.with_ema {
        let days: f32 = 7.0;
        let mut ema: f32 = value_vec[0];
        format!(
            "M0,{:.2} {}",
            graph_height - value_vec[0] * scale_h,
            value_vec
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    ema *= (days - 1.0) / days;
                    ema += v / days;
                    format!(
                        "L{:.2},{:.2}",
                        i as f32 * scale_w,
                        graph_height - ema * scale_h
                    )
                })
                .collect::<Vec<_>>()
                .join(" ")
        )
    } else {
        "".to_string()
    };
    {
        let first = cx.props.data[0].1 as f32;
        for _ in 0..6 {
            value_vec.insert(0, first);
        }
    }
    let line_average = if cx.props.with_ema {
        let average = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
        format!(
            "M0,{:.2} {}",
            graph_height - value_vec[0] * scale_h,
            value_vec
                .windows(7)
                .enumerate()
                .map(|(i, v)| {
                    format!(
                        "L{:.2},{:.2}",
                        i as f32 * scale_w,
                        graph_height - average(v) * scale_h
                    )
                })
                .collect::<Vec<_>>()
                .join(" ")
        )
    } else {
        "".to_string()
    };
    let cell_style = "display: inline-block; width: 180px; margin-left: 20px; text-align: left;";
    let value_style = "display: inline-block; width: 180px; margin-left: 20px; text-align: right;";
    cx.render(rsx!(
        hr {}
        div {
            class: "data-graph",
            svg {
                fill: "none",
                stroke_linecap: "round",
                stroke_linejoin: "round",

                view_box: "0 0 400 100",
                path {
                    stroke: "red",
                    stroke_width: "0.8",
                    d: "{line_average}"
                }
                path {
                    stroke: "green",
                    stroke_width: "0.4",
                    stroke_dasharray: "6 2",
                    d: "{line_ema}"
                }
                path {
                    stroke: "currentColor",
                    stroke_width: "1",
                    d: "{path}"
                }
            }
        }
        div {
            class: "data-table",
            cx.props.data.iter().enumerate().map(|(i, (k, v))| {
                let style = if i % 2 == 0 {
                    "background-color: #eeeeee;"
                } else {
                    "background-color: #eaeaea;"
                };
                rsx!(
                    div {
                        style: "{style}",
                        div {
                            style: "{cell_style}",
                            "{k}"
                        }
                        div {
                            style: "{value_style}",
                            "{v}"
                        }
                    }
                )
            })
        }
    ))
}
