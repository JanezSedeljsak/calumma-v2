use rayon::prelude::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct Gaussian {
    weight: f32,
    mean: [f32; 3],
    inv_cov: [[f32; 3]; 3],
    det: f32,
}

impl Gaussian {
    fn mahalanobis(&self, x: [f32; 3]) -> f32 {
        let d = [
            x[0] - self.mean[0],
            x[1] - self.mean[1],
            x[2] - self.mean[2],
        ];
        let mut m = 0.0f32;
        for i in 0..3 {
            for j in 0..3 {
                m += d[i] * self.inv_cov[i][j] * d[j];
            }
        }
        m.clamp(0.0, 500.0)
    }

    fn log_unnorm(&self, x: [f32; 3]) -> f32 {
        self.weight.max(1e-12).ln() - 0.5 * self.det.max(1e-12).ln() - 0.5 * self.mahalanobis(x)
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct Gmm {
    pub(super) components: Vec<Gaussian>,
}

impl Gmm {
    pub(super) fn neg_log_density(&self, x: [f32; 3]) -> f32 {
        if self.components.is_empty() {
            return 500.0;
        }
        let logs: Vec<f32> = self.components.iter().map(|g| g.log_unnorm(x)).collect();
        let mut max_l = f32::NEG_INFINITY;
        for &l in &logs {
            max_l = max_l.max(l);
        }
        if !max_l.is_finite() {
            return 500.0;
        }
        let mut sum = 0.0f32;
        for l in logs {
            sum += (l - max_l).exp();
        }
        let nll = -(max_l + sum.max(1e-12).ln());
        if nll.is_finite() {
            nll.clamp(0.0, 500.0)
        } else {
            500.0
        }
    }

    fn best_component(&self, x: [f32; 3]) -> usize {
        let mut best = 0usize;
        let mut best_l = f32::NEG_INFINITY;
        for (i, g) in self.components.iter().enumerate() {
            let l = g.log_unnorm(x);
            if l > best_l {
                best_l = l;
                best = i;
            }
        }
        best
    }
}

struct Lcg(u32);

impl Lcg {
    fn next_f32(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / ((1u32 << 24) as f32)
    }
}

pub(super) fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
}

fn kmeans_assignment(samples: &[[f32; 3]], k: usize, rounds: usize) -> Vec<usize> {
    if samples.is_empty() {
        return Vec::new();
    }
    let k = k.min(samples.len()).max(1);
    let mut rng = Lcg(0x5EED_1234);
    let mut centres: Vec<[f32; 3]> = Vec::with_capacity(k);
    centres.push(samples[(rng.next_f32() * samples.len() as f32) as usize % samples.len()]);
    while centres.len() < k {
        let d2: Vec<f32> = samples
            .par_iter()
            .map(|s| {
                centres
                    .iter()
                    .map(|c| dist2(*s, *c))
                    .fold(f32::INFINITY, f32::min)
            })
            .collect();
        let total: f32 = d2.iter().sum();
        if total <= f32::MIN_POSITIVE {
            break;
        }
        let mut target = rng.next_f32() * total;
        let mut chosen = samples.len() - 1;
        for (i, d) in d2.iter().enumerate() {
            target -= d;
            if target <= 0.0 {
                chosen = i;
                break;
            }
        }
        if centres.iter().any(|c| dist2(*c, samples[chosen]) <= 1e-6) {
            let mut best_i = chosen;
            let mut best_d = -1.0f32;
            for (i, d) in d2.iter().enumerate() {
                if *d > best_d && centres.iter().all(|c| dist2(*c, samples[i]) > 1e-6) {
                    best_d = *d;
                    best_i = i;
                }
            }
            if best_d <= 0.0 {
                break;
            }
            chosen = best_i;
        }
        centres.push(samples[chosen]);
    }

    let mut assignment = vec![0usize; samples.len()];
    for _ in 0..rounds {
        assign_nearest(&mut assignment, samples, &centres);
        let k_n = centres.len();
        let (sums, counts) = reduce_centroids(samples, &assignment, k_n);
        for (i, centre) in centres.iter_mut().enumerate() {
            if counts[i] > 0 {
                let n = counts[i] as f64;
                *centre = [
                    (sums[i][0] / n) as f32,
                    (sums[i][1] / n) as f32,
                    (sums[i][2] / n) as f32,
                ];
            }
        }
    }
    assignment
}

fn assign_nearest(assignment: &mut [usize], samples: &[[f32; 3]], centres: &[[f32; 3]]) {
    assignment
        .par_iter_mut()
        .zip(samples.par_iter())
        .for_each(|(slot, s)| {
            let mut best = 0usize;
            let mut best_d = f32::INFINITY;
            for (i, c) in centres.iter().enumerate() {
                let d = dist2(*s, *c);
                if d < best_d {
                    best_d = d;
                    best = i;
                }
            }
            *slot = best;
        });
}

fn reduce_centroids(
    samples: &[[f32; 3]],
    assignment: &[usize],
    k: usize,
) -> (Vec<[f64; 3]>, Vec<usize>) {
    samples
        .par_iter()
        .zip(assignment.par_iter())
        .fold(
            || (vec![[0f64; 3]; k], vec![0usize; k]),
            |mut acc, (s, &a)| {
                acc.0[a][0] += s[0] as f64;
                acc.0[a][1] += s[1] as f64;
                acc.0[a][2] += s[2] as f64;
                acc.1[a] += 1;
                acc
            },
        )
        .reduce(
            || (vec![[0f64; 3]; k], vec![0usize; k]),
            |mut a, b| {
                for i in 0..k {
                    a.0[i][0] += b.0[i][0];
                    a.0[i][1] += b.0[i][1];
                    a.0[i][2] += b.0[i][2];
                    a.1[i] += b.1[i];
                }
                a
            },
        )
}

fn invert3(m: [[f64; 3]; 3]) -> Option<([[f32; 3]; 3], f32)> {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if !det.is_finite() || det.abs() < 1e-12 {
        return None;
    }
    let inv_det = 1.0 / det;
    let mut inv = [[0f32; 3]; 3];
    inv[0][0] = ((m[1][1] * m[2][2] - m[1][2] * m[2][1]) * inv_det) as f32;
    inv[0][1] = ((m[0][2] * m[2][1] - m[0][1] * m[2][2]) * inv_det) as f32;
    inv[0][2] = ((m[0][1] * m[1][2] - m[0][2] * m[1][1]) * inv_det) as f32;
    inv[1][0] = ((m[1][2] * m[2][0] - m[1][0] * m[2][2]) * inv_det) as f32;
    inv[1][1] = ((m[0][0] * m[2][2] - m[0][2] * m[2][0]) * inv_det) as f32;
    inv[1][2] = ((m[0][2] * m[1][0] - m[0][0] * m[1][2]) * inv_det) as f32;
    inv[2][0] = ((m[1][0] * m[2][1] - m[1][1] * m[2][0]) * inv_det) as f32;
    inv[2][1] = ((m[0][1] * m[2][0] - m[0][0] * m[2][1]) * inv_det) as f32;
    inv[2][2] = ((m[0][0] * m[1][1] - m[0][1] * m[1][0]) * inv_det) as f32;
    if inv.iter().flatten().any(|v| !v.is_finite()) {
        return None;
    }
    Some((inv, det.abs() as f32))
}

fn gaussians_from_assignment(
    samples: &[[f32; 3]],
    assignment: &[usize],
    k: usize,
) -> Vec<Gaussian> {
    let total = samples.len();
    if total == 0 {
        return Vec::new();
    }
    let mut sums = vec![[0f64; 3]; k];
    let mut counts = vec![0usize; k];
    for (s, &a) in samples.iter().zip(assignment.iter()) {
        if a >= k {
            continue;
        }
        sums[a][0] += s[0] as f64;
        sums[a][1] += s[1] as f64;
        sums[a][2] += s[2] as f64;
        counts[a] += 1;
    }
    let mut means = vec![[0f32; 3]; k];
    for i in 0..k {
        if counts[i] == 0 {
            continue;
        }
        let n = counts[i] as f64;
        means[i] = [
            (sums[i][0] / n) as f32,
            (sums[i][1] / n) as f32,
            (sums[i][2] / n) as f32,
        ];
    }
    let mut covs = vec![[[0f64; 3]; 3]; k];
    for (s, &a) in samples.iter().zip(assignment.iter()) {
        if a >= k || counts[a] < 2 {
            continue;
        }
        let mean = means[a];
        let d = [
            s[0] as f64 - mean[0] as f64,
            s[1] as f64 - mean[1] as f64,
            s[2] as f64 - mean[2] as f64,
        ];
        for i in 0..3 {
            for j in 0..3 {
                covs[a][i][j] += d[i] * d[j];
            }
        }
    }
    let mut components: Vec<Gaussian> = (0..k)
        .filter(|&i| counts[i] >= 2)
        .filter_map(|i| {
            let n = counts[i] as f64;
            let mut cov = covs[i];
            for row in cov.iter_mut() {
                for c in row.iter_mut() {
                    *c /= n;
                }
            }
            let mut ridge = 1.0f64;
            loop {
                let mut regularized = cov;
                for (diag, row) in regularized.iter_mut().enumerate() {
                    row[diag] += ridge;
                }
                if let Some((inv_cov, det)) = invert3(regularized) {
                    return Some(Gaussian {
                        weight: (n / total as f64) as f32,
                        mean: means[i],
                        inv_cov,
                        det,
                    });
                }
                ridge *= 10.0;
                if ridge > 1.0e4 {
                    return None;
                }
            }
        })
        .collect();
    let wsum: f32 = components.iter().map(|g| g.weight).sum();
    if wsum > 1e-12 {
        for g in &mut components {
            g.weight /= wsum;
        }
    }
    components
}

pub(super) fn fit_gmm(samples: &[[f32; 3]], k: usize) -> Gmm {
    if samples.len() < 2 {
        return Gmm::default();
    }
    let mut assignment = kmeans_assignment(samples, k, 8);
    let mut components = gaussians_from_assignment(samples, &assignment, k);
    for _ in 0..2 {
        if components.is_empty() {
            break;
        }
        let gmm = Gmm {
            components: components.clone(),
        };
        assignment
            .par_iter_mut()
            .zip(samples.par_iter())
            .for_each(|(slot, s)| *slot = gmm.best_component(*s));
        components = gaussians_from_assignment(samples, &assignment, gmm.components.len().max(k));
        if components.is_empty() {
            components = gmm.components;
            break;
        }
    }
    Gmm { components }
}
