//! Dev tool: `cargo run --release -p tectora-drawing --example probe -- <pdf> [page x y w h dpi out.png]`
//! prints page info, and with a region renders a crop.
use std::time::Instant;
use tectora_drawing::{Drawing, RectMm, Turn};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let d = Drawing::open(std::fs::read(&a[0]).expect("read")).expect("open");
    if a.len() == 1 {
        for i in 0..d.page_count() {
            let t = Instant::now();
            let info = d.page_info(i).expect("info");
            println!("{} ({:?})", serde_json_like(&info), t.elapsed());
        }
        return;
    }
    let n = |i: usize| a[i].parse::<f64>().expect("number");
    let t = Instant::now();
    let r = d
        .render(
            n(1) as usize,
            Some(RectMm::new(n(2), n(3), n(4), n(5))),
            Some(n(6)),
            4096,
            Turn::None,
        )
        .expect("render");
    std::fs::write(&a[7], &r.png).expect("write");
    println!(
        "{}x{} px, {:.2} px/mm, {:?}",
        r.width,
        r.height,
        r.px_per_mm,
        t.elapsed()
    );
}

fn serde_json_like(i: &tectora_drawing::PageInfo) -> String {
    format!(
        "p{} {}x{} mm rot {} {:?} seg {} text {} img {} {:?}",
        i.index,
        i.width_mm,
        i.height_mm,
        i.rotate,
        i.kind,
        i.segments,
        i.text_items,
        i.images,
        i.paper
    )
}
