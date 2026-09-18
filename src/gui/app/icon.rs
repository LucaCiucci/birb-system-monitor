
// TODO build at compile time using resvg, see <https://github.com/linebender/resvg/blob/main/crates/resvg/examples/minimal.rs>
pub fn make_icon() -> egui::IconData {
    let size = 32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    // Define a few polyline segments for the graph lines
    let lines: &[&[(i32, i32)]] = &[
        &[(4, 24), (12, 20), (20, 22), (28, 12)], // CPU - blue
        &[(4, 24), (12, 22), (20, 18), (28, 16)], // Mem - green
        &[(4, 24), (12, 23), (20, 20), (28, 18)], // Net - orange
    ];
    let colors: &[(u8, u8, u8)] = &[(100, 181, 246), (129, 199, 132), (255, 183, 77)];

    for y in 0..size {
        for x in 0..size {
            let (r, g, b, a) = if x >= 4 && x <= 28 && y >= 6 && y <= 28 {
                let px = x as f32;
                let py = y as f32;
                let mut closest = f32::MAX;
                let mut best = (20u8, 20u8, 30u8);

                for (line_idx, seg) in lines.iter().enumerate() {
                    for pair in seg.windows(2) {
                        let (x1, y1) = pair[0];
                        let (x2, y2) = pair[1];
                        let dx = (x2 - x1) as f32;
                        let dy = (y2 - y1) as f32;
                        let len2 = dx * dx + dy * dy;
                        if len2 < 0.001 {
                            continue;
                        }
                        let t = ((px - x1 as f32) * dx + (py - y1 as f32) * dy) / len2;
                        let t = t.clamp(0.0, 1.0);
                        let nx = x1 as f32 + t * dx;
                        let ny = y1 as f32 + t * dy;
                        let d = ((px - nx).powi(2) + (py - ny).powi(2)).sqrt();
                        if d < closest {
                            closest = d;
                            best = colors[line_idx];
                        }
                    }
                }

                if closest < 2.5 {
                    (best.0, best.1, best.2, 255)
                } else {
                    let bg = 20 + ((y as f32 / size as f32) * 15.0) as u8;
                    (bg, bg, bg + 10, 255)
                }
            } else {
                (10, 10, 20, 255)
            };
            rgba.push(r);
            rgba.push(g);
            rgba.push(b);
            rgba.push(a);
        }
    }
    egui::IconData {
        rgba,
        width: size,
        height: size,
    }
}
