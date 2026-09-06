//! Dinic max-flow / min-cut on a directed graph with `f32` capacities. Residual arithmetic
//! uses a floor rather than exact zero because GrabCut's capacities are log-likelihoods.

const EPS: f32 = 1e-6;
const CAP_MAX: f32 = 1.0e8;

fn sanitise_cap(c: f32) -> f32 {
    if c.is_finite() && c > 0.0 {
        c.min(CAP_MAX)
    } else {
        0.0
    }
}

pub struct FlowGraph {
    head: Vec<i32>,
    next: Vec<i32>,
    to: Vec<u32>,
    cap: Vec<f32>,
    orig: Vec<f32>,
    level: Vec<i32>,
    iter: Vec<i32>,
    queue: Vec<usize>,
}

impl FlowGraph {
    pub fn new(nodes: usize) -> Self {
        Self {
            head: vec![-1; nodes],
            next: Vec::new(),
            to: Vec::new(),
            cap: Vec::new(),
            orig: Vec::new(),
            level: vec![-1; nodes],
            iter: vec![-1; nodes],
            queue: Vec::new(),
        }
    }

    pub fn with_capacity(nodes: usize, edges: usize) -> Self {
        let mut g = Self::new(nodes);
        let directed = edges.saturating_mul(2);
        g.next.reserve(directed);
        g.to.reserve(directed);
        g.cap.reserve(directed);
        g.orig.reserve(directed);
        g.queue.reserve(nodes.min(1024));
        g
    }

    pub fn node_count(&self) -> usize {
        self.head.len()
    }

    pub fn add_edge(&mut self, from: usize, to: usize, cap: f32, rev_cap: f32) -> usize {
        let e = self.to.len();
        let cap = sanitise_cap(cap);
        let rev_cap = sanitise_cap(rev_cap);
        self.to.push(to as u32);
        self.cap.push(cap);
        self.orig.push(cap);
        self.next.push(self.head[from]);
        self.head[from] = e as i32;

        self.to.push(from as u32);
        self.cap.push(rev_cap);
        self.orig.push(rev_cap);
        self.next.push(self.head[to]);
        self.head[to] = (e + 1) as i32;
        e
    }

    pub fn set_capacity(&mut self, edge: usize, cap: f32, rev_cap: f32) {
        let cap = sanitise_cap(cap);
        let rev_cap = sanitise_cap(rev_cap);
        self.orig[edge] = cap;
        self.cap[edge] = cap;
        self.orig[edge ^ 1] = rev_cap;
        self.cap[edge ^ 1] = rev_cap;
    }

    pub fn reset_flow(&mut self) {
        self.cap.copy_from_slice(&self.orig);
    }

    fn build_levels(&mut self, s: usize, t: usize) -> bool {
        self.level.fill(-1);
        self.queue.clear();
        self.level[s] = 0;
        self.queue.push(s);
        let mut qh = 0;
        while qh < self.queue.len() {
            let v = self.queue[qh];
            qh += 1;
            if self.level[t] >= 0 && self.level[v] >= self.level[t] {
                continue;
            }
            let mut e = self.head[v];
            while e >= 0 {
                let ei = e as usize;
                let u = self.to[ei] as usize;
                if self.cap[ei] > EPS && self.level[u] < 0 {
                    self.level[u] = self.level[v] + 1;
                    self.queue.push(u);
                }
                e = self.next[ei];
            }
        }
        self.level[t] >= 0
    }

    fn augment(&mut self, s: usize, t: usize) -> f32 {
        let mut path: Vec<usize> = Vec::new();
        let mut v = s;
        loop {
            if v == t {
                let f = path
                    .iter()
                    .map(|&e| self.cap[e])
                    .fold(f32::INFINITY, f32::min);
                for &e in &path {
                    self.cap[e] -= f;
                    self.cap[e ^ 1] += f;
                }
                return f;
            }
            let mut advanced = false;
            while self.iter[v] >= 0 {
                let ei = self.iter[v] as usize;
                let u = self.to[ei] as usize;
                if self.cap[ei] > EPS && self.level[v] < self.level[u] {
                    path.push(ei);
                    v = u;
                    advanced = true;
                    break;
                }
                self.iter[v] = self.next[ei];
            }
            if !advanced {
                if v == s {
                    return 0.0;
                }
                self.level[v] = -1;
                let Some(ei) = path.pop() else {
                    return 0.0;
                };
                v = self.to[ei ^ 1] as usize;
                self.iter[v] = self.next[ei];
            }
        }
    }

    pub fn max_flow(&mut self, s: usize, t: usize) -> f32 {
        if s == t {
            return 0.0;
        }
        let mut total = 0.0;
        while self.build_levels(s, t) {
            self.iter.copy_from_slice(&self.head);
            loop {
                let f = self.augment(s, t);
                if f <= EPS {
                    break;
                }
                total += f;
            }
        }
        total
    }

    pub fn source_side(&self, s: usize) -> Vec<bool> {
        let mut seen = vec![false; self.head.len()];
        let mut queue = Vec::with_capacity(self.head.len().min(1024));
        seen[s] = true;
        queue.push(s);
        let mut qh = 0;
        while qh < queue.len() {
            let v = queue[qh];
            qh += 1;
            let mut e = self.head[v];
            while e >= 0 {
                let ei = e as usize;
                let u = self.to[ei] as usize;
                if self.cap[ei] > EPS && !seen[u] {
                    seen[u] = true;
                    queue.push(u);
                }
                e = self.next[ei];
            }
        }
        seen
    }
}
