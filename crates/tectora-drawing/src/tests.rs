use super::*;

/// A minimal one-page PDF with the given content stream, page size in pt
/// and optional `/Rotate`.
fn pdf(content: &str, w: f64, h: f64, rotate: u32) -> Vec<u8> {
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Rotate {rotate} /Contents 4 0 R /Resources << >> >>"
        ),
        format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        ),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objs.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// 1 mm in pt.
const MM: f64 = 1.0 / MM_PER_PT;

/// An A4 portrait sheet with a 100 × 50 mm rectangle whose top left corner is
/// 20 mm from the left and 30 mm from the top of the sheet.
fn rect_sheet(rotate: u32) -> Drawing {
    let (w, h) = (210.0 * MM, 297.0 * MM);
    // PDF y is up: the top edge 30 mm below the sheet top is at h - 30 mm.
    let c = format!(
        "1 w {x} {y} {rw} {rh} re S",
        x = 20.0 * MM,
        y = h - 80.0 * MM,
        rw = 100.0 * MM,
        rh = 50.0 * MM
    );
    Drawing::open(pdf(&c, w, h, rotate)).expect("valid pdf")
}

#[test]
fn rejects_non_pdf() {
    assert!(matches!(
        Drawing::open(b"hello".to_vec()),
        Err(DrawingError::NotPdf)
    ));
}

#[test]
fn page_info_reports_size_paper_and_kind() {
    let d = rect_sheet(0);
    assert_eq!(d.page_count(), 1);
    let i = d.page_info(0).unwrap();
    assert!((i.width_mm - 210.0).abs() < 0.1 && (i.height_mm - 297.0).abs() < 0.1);
    assert_eq!(i.paper.as_deref(), Some("A4"));
    assert_eq!(i.segments, 4);
    assert_eq!(
        i.kind,
        PageKind::Empty,
        "four lines are too few to be a drawing"
    );
    assert!(matches!(d.page_info(1), Err(DrawingError::NoPage(1, 1))));
}

#[test]
fn lines_are_in_sheet_mm_with_y_down() {
    let d = rect_sheet(0);
    let c = d.content(0).unwrap();
    let mut lens: Vec<f64> = c.segments.iter().map(|s| round(s.length(), 3)).collect();
    lens.sort_by(f64::total_cmp);
    assert_eq!(lens, vec![50.0, 50.0, 100.0, 100.0]);
    let top = c
        .segments
        .iter()
        .find(|s| (s.a[1] - 30.0).abs() < 1e-4 && (s.b[1] - 30.0).abs() < 1e-4)
        .unwrap_or_else(|| panic!("top edge 30 mm below the sheet top: {:?}", c.segments));
    assert!((top.a[0].min(top.b[0]) - 20.0).abs() < 1e-4);
    assert!(
        (top.width - 1.0 * MM_PER_PT).abs() < 1e-9,
        "stroke width 1 pt"
    );
}

#[test]
fn rotated_page_uses_displayed_orientation() {
    let d = rect_sheet(90);
    let (w, h) = d.page_size_mm(0).unwrap();
    assert!((w - 297.0).abs() < 0.1 && (h - 210.0).abs() < 0.1);
    let c = d.content(0).unwrap();
    for s in &c.segments {
        for p in [s.a, s.b] {
            assert!(p[0] >= -1e-6 && p[0] <= w + 1e-6 && p[1] >= -1e-6 && p[1] <= h + 1e-6);
        }
    }
    // Turned clockwise, the 100 mm edges become vertical.
    let long: Vec<_> = c
        .segments
        .iter()
        .filter(|s| (s.length() - 100.0).abs() < 1e-6)
        .collect();
    assert_eq!(long.len(), 2);
    assert!(long.iter().all(|s| (s.angle_deg() - 90.0).abs() < 1e-6));
    assert_eq!(d.page_info(0).unwrap().rotate, 90);
}

#[test]
fn measure_snaps_to_corners() {
    let c = rect_sheet(0).content(0).unwrap();
    let m = c.measure([20.6, 29.5], [119.4, 30.7], 1.5);
    assert_eq!(m.snapped, [true, true]);
    assert!((m.sheet_mm - 100.0).abs() < 1e-4, "{m:?}");
    let far = c.measure([60.0, 60.0], [61.0, 60.0], 1.0);
    assert_eq!(far.snapped, [false, false]);
    assert!((far.sheet_mm - 1.0).abs() < 1e-9);
}

#[test]
fn snap_finds_line_crossings() {
    let segs = vec![
        Segment {
            a: [0.0, 5.0],
            b: [10.0, 5.0],
            width: 0.1,
            dashed: false,
        },
        Segment {
            a: [4.0, 0.0],
            b: [4.0, 10.0],
            width: 0.1,
            dashed: false,
        },
    ];
    let c = PageContent {
        segments: segs,
        ..Default::default()
    };
    let p = c.snap([4.3, 5.2], 1.0).unwrap();
    assert!((p[0] - 4.0).abs() < 1e-9 && (p[1] - 5.0).abs() < 1e-9);
}

#[test]
fn merge_joins_touching_collinear_pieces_only() {
    let s = |a: [f64; 2], b: [f64; 2]| Segment {
        a,
        b,
        width: 0.1,
        dashed: false,
    };
    let merged = merge_collinear(vec![
        s([0.0, 0.0], [10.0, 0.0]),
        s([10.0, 0.0], [25.0, 0.0]),
        s([30.0, 0.0], [40.0, 0.0]),
        s([0.0, 3.0], [10.0, 3.0]),
    ]);
    let mut lens: Vec<f64> = merged.iter().map(|m| round(m.length(), 6)).collect();
    lens.sort_by(f64::total_cmp);
    assert_eq!(lens, vec![10.0, 10.0, 25.0]);
}

#[test]
fn lines_filter_by_region_and_length() {
    let c = rect_sheet(0).content(0).unwrap();
    let all = c.lines(None, 0.0, 100);
    assert_eq!(all.len(), 4);
    assert!((all[0].length() - 100.0).abs() < 1e-6, "longest first");
    let left = c.lines(Some(RectMm::new(10.0, 25.0, 15.0, 60.0)), 60.0, 100);
    assert_eq!(left.len(), 2, "both 100 mm edges touch the left corners");
}

#[test]
fn render_maps_pixels_back_to_the_sheet() {
    let d = rect_sheet(0);
    let region = RectMm::new(10.0, 20.0, 120.0, 70.0);
    for turn in [Turn::None, Turn::Cw90, Turn::Cw180, Turn::Cw270] {
        let r = d
            .render(0, Some(region), Some(254.0), 4096, turn, None)
            .unwrap();
        assert!(r.png.starts_with(b"\x89PNG"));
        assert!((r.px_per_mm - 10.0).abs() < 1e-9);
        let (w, h) = if matches!(turn, Turn::Cw90 | Turn::Cw270) {
            (700, 1200)
        } else {
            (1200, 700)
        };
        assert_eq!((r.width, r.height), (w, h), "{turn:?}");
        // The image corners map to the region corners.
        let corners = [r.sheet_point(0.0, 0.0), r.sheet_point(w as f64, h as f64)];
        let xs = [
            corners[0][0].min(corners[1][0]),
            corners[0][0].max(corners[1][0]),
        ];
        let ys = [
            corners[0][1].min(corners[1][1]),
            corners[0][1].max(corners[1][1]),
        ];
        assert!(
            (xs[0] - 10.0).abs() < 1e-9 && (xs[1] - 130.0).abs() < 1e-9,
            "{turn:?}"
        );
        assert!(
            (ys[0] - 20.0).abs() < 1e-9 && (ys[1] - 90.0).abs() < 1e-9,
            "{turn:?}"
        );
    }
}

#[test]
fn render_clamps_resolution_and_region() {
    let d = rect_sheet(0);
    let r = d
        .render(0, None, Some(5000.0), 1000, Turn::None, None)
        .unwrap();
    assert!(r.width.max(r.height) <= 1000);
    assert!(matches!(
        d.render(
            0,
            Some(RectMm::new(500.0, 500.0, 10.0, 10.0)),
            None,
            1000,
            Turn::None,
            None
        ),
        Err(DrawingError::EmptyRegion)
    ));
    let tiny = d
        .render(
            0,
            Some(RectMm::new(0.0, 0.0, 5.0, 5.0)),
            Some(10.0),
            4096,
            Turn::None,
            None,
        )
        .unwrap();
    assert!(
        tiny.width >= MIN_RENDER_PX,
        "small crops are enlarged so models can read them"
    );
}

#[test]
fn scale_and_paper_parsing() {
    assert_eq!(parse_scale("1:50"), Some(50.0));
    assert_eq!(parse_scale("M 1:100"), Some(100.0));
    assert_eq!(parse_scale(" 1 : 25 "), Some(25.0));
    assert_eq!(parse_scale("2:50"), None);
    assert_eq!(parse_scale("abc"), None);
    assert_eq!(paper_name(420.0, 297.0).as_deref(), Some("A3"));
    assert_eq!(paper_name(600.0, 670.0), None);
}

/// A horizontal dimension line 30 mm below the top of an A4 sheet from
/// x = 20 to 120 mm with extension lines (4 mm, from below) at 20, 50 and
/// 120 mm, and two short "text" strokes touching it from above.
fn dimension_sheet() -> Drawing {
    let (w, h) = (210.0 * MM, 297.0 * MM);
    let y = h - 30.0 * MM;
    let mut c = format!("0.2 w {} {y} m {} {y} l S", 18.0 * MM, 122.0 * MM);
    for x in [20.0, 50.0, 120.0] {
        c += &format!(
            " {} {} m {} {} l S",
            x * MM,
            y - 4.0 * MM,
            x * MM,
            y + 0.1 * MM
        );
    }
    for x in [33.0, 34.5] {
        c += &format!(" {} {y} m {} {} l S", x * MM, x * MM, y + 1.4 * MM);
    }
    Drawing::open(pdf(&c, w, h, 0)).expect("valid pdf")
}

#[test]
fn dimensions_are_found_from_extension_lines() {
    let c = dimension_sheet().content(0).unwrap();
    let chains = c.dimensions(None);
    assert_eq!(chains.len(), 1, "{chains:?}");
    let spans: Vec<f64> = chains[0]
        .dimensions
        .iter()
        .map(|d| round(d.sheet_mm, 3))
        .collect();
    assert_eq!(
        spans,
        vec![30.0, 70.0],
        "text strokes from the other side are not marks"
    );
    assert!((chains[0].sheet_mm - 100.0).abs() < 1e-3);
    let mid = chains[0].dimensions[0].mid;
    assert!((mid[0] - 35.0).abs() < 1e-3 && (mid[1] - 30.0).abs() < 1e-3);
    // A region away from the chain finds nothing.
    assert!(
        c.dimensions(Some(RectMm::new(150.0, 150.0, 40.0, 40.0)))
            .is_empty()
    );
}

#[test]
fn trace_snaps_a_rough_polygon_to_the_corners() {
    let c = rect_sheet(0).content(0).unwrap();
    let t = c.trace(
        &[[21.0, 30.8], [119.2, 29.4], [120.6, 80.9], [19.5, 79.3]],
        1.5,
    );
    assert_eq!(t.snapped, vec![true; 4]);
    assert_eq!(t.edges_sheet_mm, vec![100.0, 50.0, 100.0, 50.0]);
    assert!((t.area_sheet_mm2 - 5000.0).abs() < 1e-6);
    assert!((t.points[2][0] - 120.0).abs() < 1e-3 && (t.points[2][1] - 80.0).abs() < 1e-3);
    let far = c.trace(&[[60.0, 60.0], [70.0, 60.0], [70.0, 70.0]], 1.0);
    assert_eq!(far.snapped, vec![false; 3]);
}

#[test]
fn grid_is_drawn_on_multiples_of_the_step() {
    let d = rect_sheet(0);
    let plain = d
        .render(
            0,
            Some(RectMm::new(0.0, 0.0, 100.0, 100.0)),
            Some(254.0),
            4096,
            Turn::None,
            None,
        )
        .unwrap();
    let grid = d
        .render(
            0,
            Some(RectMm::new(0.0, 0.0, 100.0, 100.0)),
            Some(254.0),
            4096,
            Turn::None,
            Some(10.0),
        )
        .unwrap();
    assert_eq!((plain.width, plain.height), (grid.width, grid.height));
    assert_ne!(plain.png, grid.png, "the grid changes the image");
    assert_eq!(grid_auto_step(297.0), 20.0);
    assert_eq!(grid_auto_step(40.0), 5.0);
}

/// A square roof outline filled with 45° hatching that runs out at the edges
/// near each corner, as on hatched roof plans.
fn hatched_square() -> PageContent {
    let s = |a: [f64; 2], b: [f64; 2]| Segment {
        a,
        b,
        width: 0.1,
        dashed: false,
    };
    let mut segs = vec![
        s([10.0, 10.0], [60.0, 10.0]),
        s([60.0, 10.0], [60.0, 60.0]),
        s([60.0, 60.0], [10.0, 60.0]),
        s([10.0, 60.0], [10.0, 10.0]),
    ];
    // Hatching every 1.3 mm, clipped to the square.
    let mut k = 0.3;
    while k < 100.0 {
        let (a, b) = if k <= 50.0 {
            ([10.0, 10.0 + k], [10.0 + k, 10.0])
        } else {
            ([10.0 + k - 50.0, 60.0], [60.0, 10.0 + k - 50.0])
        };
        segs.push(s(a, b));
        k += 1.3;
    }
    PageContent {
        segments: segs,
        ..Default::default()
    }
}

#[test]
fn trace_snaps_to_the_outline_not_to_hatching() {
    let c = hatched_square();
    // Rough corners about 1 mm off, closer to hatch crossings than to the corner.
    let t = c.trace(
        &[[10.9, 10.4], [59.2, 10.9], [59.4, 59.1], [10.6, 59.2]],
        1.5,
    );
    assert_eq!(t.snapped, vec![true; 4]);
    assert_eq!(t.edges_sheet_mm, vec![50.0, 50.0, 50.0, 50.0]);
    assert_eq!(t.edges_angle_deg, vec![0.0, 90.0, 0.0, 90.0]);
    assert_eq!(t.edges_drawn, vec![1.0; 4]);
}

#[test]
fn trace_reports_edges_off_the_drawn_lines() {
    let c = hatched_square();
    // The third corner is far off: with snapping off it stays where it is, and
    // the two edges to it run across the square, not along its outline.
    let t = c.trace(
        &[[10.0, 10.0], [60.0, 10.0], [45.0, 45.0], [10.0, 60.0]],
        0.0,
    );
    assert_eq!(t.edges_drawn[0], 1.0);
    assert_eq!(t.edges_drawn[3], 1.0);
    assert!(
        t.edges_drawn[1] < 0.1 && t.edges_drawn[2] < 0.1,
        "{:?}",
        t.edges_drawn
    );
}

#[test]
fn lines_along_keeps_only_the_given_directions() {
    let c = hatched_square();
    assert_eq!(c.lines(None, 2.0, 4).len(), 4);
    assert!(
        c.lines(None, 2.0, 4)
            .iter()
            .any(|l| (l.angle_deg() - 45.0).abs() < 1.0 || (l.angle_deg() - 135.0).abs() < 1.0),
        "longest first: the hatching crowds the outline out"
    );
    let square = c.lines_along(None, 2.0, 4, &[0.0, 90.0]);
    assert_eq!(square.len(), 4);
    assert!(square.iter().all(|l| (l.length() - 50.0).abs() < 1e-9));
}

fn glyph(text: &str, x: f64, y: f64, angle_deg: f64) -> TextItem {
    TextItem {
        text: text.into(),
        at: [x, y],
        size: 1.48,
        angle_deg,
        end: [x, y],
    }
}

#[test]
fn glyph_runs_join_into_labels() {
    // A vertical CAD label written glyph by glyph (baseline downwards on
    // the sheet), then a second label elsewhere.
    let mut items: Vec<TextItem> = [
        ("o", 141.24),
        ("k", 142.21),
        ("a", 143.1),
        ("p", 144.03),
        ("+", 147.21),
        ("6", 148.22),
        (",", 149.2),
        ("8", 149.7),
        ("8", 150.64),
        ("0", 151.57),
    ]
    .iter()
    .map(|(t, y)| glyph(t, 142.37, *y, 90.0))
    .collect();
    items.push(glyph("4", 168.19, 147.25, -0.4));
    items.push(glyph("0°", 169.16, 147.24, -0.4));
    // Same baseline but far away: a separate label.
    items.push(glyph("1290", 190.0, 147.24, -0.4));
    let joined = join_text_runs(items);
    let texts: Vec<&str> = joined.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(texts, ["okap +6,880", "40°", "1290"]);
    assert_eq!(joined[0].at, [142.37, 141.24]);
}
