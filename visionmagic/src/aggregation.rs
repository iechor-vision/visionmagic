//! Processor to remove small clusters by merging into larger ones
use std::collections::HashSet;
use visioniechor::{Color, ColorImage};
use visioniechor::color_clusters::Clusters;

use crate::pipeline::Processor as ProcessorTrait;

#[derive(Default)]
pub struct Processor {
    params: Params,
    width: u32,
    height: u32,
    indices: Vec<AggregateIndex>,
    aggregates: Vec<Aggregate>,
    counter: usize,
}

/// [`Clusters`]
pub type Input = Clusters;

/// [`ColorImage`]
pub type Output = ColorImage;

pub struct Params {
    /// Allowed color difference between shapes in same aggregate
    pub deviation: f64,
    /// Minimum patch size in area
    pub min_size: u32,
}

struct Aggregate {
    indices: Vec<u32>,
    color: Color,
}

#[derive(Copy, Clone, Default, Eq, Ord, Hash, PartialEq, PartialOrd)]
struct AggregateIndex(pub usize);

const ZERO: AggregateIndex = AggregateIndex(0);

impl Default for Params {
    fn default() -> Self {
        Self {
            deviation: 1.0,
            min_size: 64 * 64,
        }
    }
}

impl ProcessorTrait for Processor {

    type Input = Input;
    type Output = Output;
    type Params = Params;

    fn new() -> Self {
        Self::default()
    }

    fn config(&mut self, params: Params) -> bool {
        self.params = params;
        if self.counter != 0 {
            panic!("Aggregate cannot be reconfigured");
        }
        true
    }

    fn input(&mut self, input: Input) -> bool {
        let view = input.view();
        self.counter = 0;
        self.width = view.width; 
        self.height = view.height;
        self.indices = vec![ZERO; view.cluster_indices.len()];
        self.aggregates.push(Aggregate {
            indices: Vec::new(),
            color: Color::new(0,0,0),
        });
        for cluster in view.iter() {
            self.aggregates.push(Aggregate {
                indices: cluster.indices.clone(),
                color: cluster.residue_color(),
            });
            let myindex = AggregateIndex(self.aggregates.len() - 1);
            for idx in cluster.indices.iter() {
                self.indices[*idx as usize] = myindex;
            }
        }
        true
    }

    fn tick(&mut self) -> bool {
        if self.counter < self.aggregates.len() {
            let myselfi = AggregateIndex(self.counter);
            let myself = self.get_agg(myselfi);
            if myself.area() > 0 {
                let mut votes: Vec<(AggregateIndex, i32)> = self.neighbours_of(myselfi).iter().map(|otheri| {
                    let other = self.get_agg(*otheri);
                    (*otheri, Self::color_distance(myself, other))
                }).collect();
                votes.sort_by_key(|v| v.1);
                if !votes.is_empty() {
                    let diff = votes[0].1 as f64 / 10000.0;
                    if  (myself.area() < self.params.min_size as usize / 16) ||
                        (diff < self.params.deviation && myself.area() < self.params.min_size as usize) ||
                        (diff < self.params.deviation * 2.0 && myself.area() < self.params.min_size as usize / 4) ||
                        (diff < self.params.deviation / 2.0 && myself.area() < self.params.min_size as usize * 4) || 
                        (diff < self.params.deviation / 4.0) {
                        self.merge_into(myselfi, votes[0].0);
                    }
                }
            }
            self.counter += 1;
            false
        } else {
            true
        }
    }

    fn progress(&self) -> u32 {
        100
    }

    /// to be called once only after process ends
    fn output(&mut self) -> Output {
        let mut image = ColorImage::new_w_h(self.width as usize, self.height as usize);
        for agg in self.aggregates.iter() {
            for px in agg.indices.iter() {
                let x = px % self.width;
                let y = px / self.width;
                image.set_pixel(x as usize, y as usize, &agg.color);
            }
        }
        image
    }

}

impl Processor {
    fn merge_into(&mut self, myselfi: AggregateIndex, otheri: AggregateIndex) {
        for idx in self.aggregates[myselfi.0 as usize].indices.iter() {
            self.indices[*idx as usize] = otheri;
        }
        let mut indices = std::mem::take(&mut self.get_agg_mut(myselfi).indices);
        self.get_agg_mut(otheri).indices.append(&mut indices);
    }

    fn color_distance(myself: &Aggregate, other: &Aggregate) -> i32 {
        let mycolor = myself.color;
        let otcolor = other.color;
        (10000.0 * Self::color_diff_hsv(mycolor, otcolor)) as i32
    }

    fn color_diff_hsv(a: Color, b: Color) -> f64 {
        let a = a.to_hsv();
        let b = b.to_hsv();
        return 1.5 * wrap(a.h, b.h) + 1.25 * (a.v - b.v).abs() + 0.75 * (a.s - b.s).abs();

        fn wrap(x: f64, y: f64) -> f64 {
            let d = (x - y).abs();
            if d < 0.5 {
                d
            } else {
                1.0 - d
            }
        }
    }

    fn get_agg(&self, index: AggregateIndex) -> &Aggregate {
        &self.aggregates[index.0 as usize]
    }

    fn get_agg_mut(&mut self, index: AggregateIndex) -> &mut Aggregate {
        &mut self.aggregates[index.0 as usize]
    }

    fn neighbours_of(&self, myselfi: AggregateIndex) -> Vec<AggregateIndex> {
        let myself = self.get_agg(myselfi);
        let mut neighbours = HashSet::new();

        for &i in myself.indices.iter() {
            let x = i % self.width;
            let y = i / self.width;

            for k in 0..4 {
                let index = match k {
                    0 => if y > 0 { self.indices[(self.width * (y - 1) + x) as usize] } else { ZERO },
                    1 => if y < self.height - 1 { self.indices[(self.width * (y + 1) + x) as usize] } else { ZERO },
                    2 => if x > 0 { self.indices[(self.width * y + (x - 1)) as usize] } else { ZERO },
                    3 => if x < self.width - 1 { self.indices[(self.width * y + (x + 1)) as usize] } else { ZERO },
                    _ => unreachable!(),
                };
                if index != ZERO && index != myselfi {
                    neighbours.insert(index);
                }
            }
        }

        let mut list: Vec<AggregateIndex> = neighbours.into_iter().collect();
        list.sort();
        list
    }
}

impl Aggregate {
    pub fn area(&self) -> usize {
        self.indices.len()
    }
}