use std::ops::RangeInclusive;
use std::sync::Arc;

use egui::emath;

use egui::vec2;
use egui::DragValue;
use egui::Widget;

pub struct DragVector<'a, const D: usize> {
    value: [DragValue<'a>; D],
}

#[allow(unused)]
impl<'a, const D: usize> DragVector<'a, D> {
    pub fn new<Num: emath::Numeric>(value: [&'a mut Num; D]) -> Self {
        Self {
            value: value.map(|v| DragValue::new(v)),
        }
    }

    fn map(self, mut f: impl FnMut(DragValue<'a>) -> DragValue<'a>) -> Self {
        Self {
            value: self.value.map(f),
        }
    }

    fn map_indexed(self, mut f: impl FnMut(usize, DragValue<'a>) -> DragValue<'a>) -> Self {
        let mut i = 0;
        Self {
            value: self.value.map(|v| {
                let v = f(i, v);
                i += 1;
                v
            }),
        }
    }

    /// How much the value changes when dragged one point (logical pixel).
    ///
    /// Should be finite and greater than zero.
    #[inline]
    pub fn speed(mut self, speed: impl Into<f64>) -> Self {
        let speed = speed.into();
        self.map(|v| v.speed(speed))
    }

    /// Clamp incoming and outgoing values to this range.
    #[inline]
    pub fn clamp_range<Num: emath::Numeric>(mut self, clamp_range: RangeInclusive<Num>) -> Self {
        self.map(|v| v.clamp_range(clamp_range.clone()))
    }

    /// Show a prefix before the number, e.g. "x: "
    #[inline]
    pub fn prefix(mut self, prefix: [impl ToString; D]) -> Self {
        self.map_indexed(|i, v| v.prefix(prefix[i].to_string()))
    }

    /// Add a suffix to the number, this can be e.g. a unit ("°" or " m")
    #[inline]
    pub fn suffix(mut self, suffix: [impl ToString; D]) -> Self {
        self.map_indexed(|i, v| v.suffix(suffix[i].to_string()))
    }

    /// Set a minimum number of decimals to display.
    /// Normally you don't need to pick a precision, as the slider will intelligently pick a precision for you.
    /// Regardless of precision the slider will use "smart aim" to help the user select nice, round values.
    #[inline]
    pub fn min_decimals(mut self, min_decimals: usize) -> Self {
        self.map(|v| v.min_decimals(min_decimals))
    }

    // TODO(emilk): we should also have a "max precision".
    /// Set a maximum number of decimals to display.
    /// Values will also be rounded to this number of decimals.
    /// Normally you don't need to pick a precision, as the slider will intelligently pick a precision for you.
    /// Regardless of precision the slider will use "smart aim" to help the user select nice, round values.
    #[inline]
    pub fn max_decimals(mut self, max_decimals: usize) -> Self {
        self.map(|v| v.max_decimals(max_decimals))
    }

    #[inline]
    pub fn max_decimals_opt(mut self, max_decimals: Option<usize>) -> Self {
        self.map(|v| v.max_decimals_opt(max_decimals))
    }

    /// Set an exact number of decimals to display.
    /// Values will also be rounded to this number of decimals.
    /// Normally you don't need to pick a precision, as the slider will intelligently pick a precision for you.
    /// Regardless of precision the slider will use "smart aim" to help the user select nice, round values.
    #[inline]
    pub fn fixed_decimals(mut self, num_decimals: usize) -> Self {
        self.map(|v| v.fixed_decimals(num_decimals))
    }

    /// Set custom formatter defining how numbers are converted into text.
    ///
    /// A custom formatter takes a `f64` for the numeric value and a `RangeInclusive<usize>` representing
    /// the decimal range i.e. minimum and maximum number of decimal places shown.
    ///
    /// See also: [`DragValue::custom_parser`]
    ///
    /// ```
    /// # egui::__run_test_ui(|ui| {
    /// # let mut my_i32: i32 = 0;
    /// ui.add(egui::DragValue::new(&mut my_i32)
    ///     .clamp_range(0..=((60 * 60 * 24) - 1))
    ///     .custom_formatter(|n, _| {
    ///         let n = n as i32;
    ///         let hours = n / (60 * 60);
    ///         let mins = (n / 60) % 60;
    ///         let secs = n % 60;
    ///         format!("{hours:02}:{mins:02}:{secs:02}")
    ///     })
    ///     .custom_parser(|s| {
    ///         let parts: Vec<&str> = s.split(':').collect();
    ///         if parts.len() == 3 {
    ///             parts[0].parse::<i32>().and_then(|h| {
    ///                 parts[1].parse::<i32>().and_then(|m| {
    ///                     parts[2].parse::<i32>().map(|s| {
    ///                         ((h * 60 * 60) + (m * 60) + s) as f64
    ///                     })
    ///                 })
    ///             })
    ///             .ok()
    ///         } else {
    ///             None
    ///         }
    ///     }));
    /// # });
    /// ```
    pub fn custom_formatter(
        mut self,
        formatter: impl 'a + Fn(f64, RangeInclusive<usize>) -> String,
    ) -> Self {
        let formatter = Arc::new(formatter);
        self.map(|v| {
            let formatter = formatter.clone();
            v.custom_formatter(move |n, r| formatter(n, r))
        })
    }

    /// Set custom parser defining how the text input is parsed into a number.
    ///
    /// A custom parser takes an `&str` to parse into a number and returns a `f64` if it was successfully parsed
    /// or `None` otherwise.
    ///
    /// See also: [`DragValue::custom_formatter`]
    ///
    /// ```
    /// # egui::__run_test_ui(|ui| {
    /// # let mut my_i32: i32 = 0;
    /// ui.add(egui::DragValue::new(&mut my_i32)
    ///     .clamp_range(0..=((60 * 60 * 24) - 1))
    ///     .custom_formatter(|n, _| {
    ///         let n = n as i32;
    ///         let hours = n / (60 * 60);
    ///         let mins = (n / 60) % 60;
    ///         let secs = n % 60;
    ///         format!("{hours:02}:{mins:02}:{secs:02}")
    ///     })
    ///     .custom_parser(|s| {
    ///         let parts: Vec<&str> = s.split(':').collect();
    ///         if parts.len() == 3 {
    ///             parts[0].parse::<i32>().and_then(|h| {
    ///                 parts[1].parse::<i32>().and_then(|m| {
    ///                     parts[2].parse::<i32>().map(|s| {
    ///                         ((h * 60 * 60) + (m * 60) + s) as f64
    ///                     })
    ///                 })
    ///             })
    ///             .ok()
    ///         } else {
    ///             None
    ///         }
    ///     }));
    /// # });
    /// ```
    #[inline]
    pub fn custom_parser(mut self, parser: impl 'a + Fn(&str) -> Option<f64>) -> Self {
        let parser = Arc::new(parser);
        self.map(|v| {
            let parser = parser.clone();
            v.custom_parser(move |i| parser(i))
        })
    }

    /// Set `custom_formatter` and `custom_parser` to display and parse numbers as binary integers. Floating point
    /// numbers are *not* supported.
    ///
    /// `min_width` specifies the minimum number of displayed digits; if the number is shorter than this, it will be
    /// prefixed with additional 0s to match `min_width`.
    ///
    /// If `twos_complement` is true, negative values will be displayed as the 2's complement representation. Otherwise
    /// they will be prefixed with a '-' sign.
    ///
    /// # Panics
    ///
    /// Panics if `min_width` is 0.
    ///
    /// ```
    /// # egui::__run_test_ui(|ui| {
    /// # let mut my_i32: i32 = 0;
    /// ui.add(egui::DragValue::new(&mut my_i32).binary(64, false));
    /// # });
    /// ```
    pub fn binary(mut self, min_width: usize, twos_complement: bool) -> Self {
        self.map(|v| v.binary(min_width, twos_complement))
    }

    /// Set `custom_formatter` and `custom_parser` to display and parse numbers as octal integers. Floating point
    /// numbers are *not* supported.
    ///
    /// `min_width` specifies the minimum number of displayed digits; if the number is shorter than this, it will be
    /// prefixed with additional 0s to match `min_width`.
    ///
    /// If `twos_complement` is true, negative values will be displayed as the 2's complement representation. Otherwise
    /// they will be prefixed with a '-' sign.
    ///
    /// # Panics
    ///
    /// Panics if `min_width` is 0.
    ///
    /// ```
    /// # egui::__run_test_ui(|ui| {
    /// # let mut my_i32: i32 = 0;
    /// ui.add(egui::DragValue::new(&mut my_i32).octal(22, false));
    /// # });
    /// ```
    pub fn octal(mut self, min_width: usize, twos_complement: bool) -> Self {
        self.map(|v| v.octal(min_width, twos_complement))
    }

    /// Set `custom_formatter` and `custom_parser` to display and parse numbers as hexadecimal integers. Floating point
    /// numbers are *not* supported.
    ///
    /// `min_width` specifies the minimum number of displayed digits; if the number is shorter than this, it will be
    /// prefixed with additional 0s to match `min_width`.
    ///
    /// If `twos_complement` is true, negative values will be displayed as the 2's complement representation. Otherwise
    /// they will be prefixed with a '-' sign.
    ///
    /// # Panics
    ///
    /// Panics if `min_width` is 0.
    ///
    /// ```
    /// # egui::__run_test_ui(|ui| {
    /// # let mut my_i32: i32 = 0;
    /// ui.add(egui::DragValue::new(&mut my_i32).hexadecimal(16, false, true));
    /// # });
    /// ```
    pub fn hexadecimal(mut self, min_width: usize, twos_complement: bool, upper: bool) -> Self {
        self.map(|v| v.hexadecimal(min_width, twos_complement, upper))
    }

    /// Update the value on each key press when text-editing the value.
    ///
    /// Default: `true`.
    /// If `false`, the value will only be updated when user presses enter or deselects the value.
    #[inline]
    pub fn update_while_editing(mut self, update: bool) -> Self {
        self.map(|v| v.update_while_editing(update))
    }
}

impl<'a, const D: usize> Widget for DragVector<'a, D> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;

            for (i, drag_value) in self.value.into_iter().enumerate() {
                let sub = match i {
                    i if i == D - 1 => 0.0,
                    _ => 1.0,
                };
                ui.allocate_ui_with_layout(
                    vec2(
                        ui.available_width() / ((D - i) as f32) - sub,
                        ui.available_height(),
                    ),
                    ui.layout().with_main_justify(true),
                    |ui| {
                        ui.add(drag_value);
                    },
                );
            }
        })
        .response
    }
}
