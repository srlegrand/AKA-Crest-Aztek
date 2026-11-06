//! NURBS curves as Maya stores them (non-rational): degree, form, control vertices and a knot vector of
//! `cvs + degree - 1` values. Periodic curves repeat their first `degree` CVs at the end. Plus the operations Maya's curve
//! commands build on: evaluation, edit points, interpolation, rebuild, reverse, closing, knot insertion, splitting.

use glam::DVec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Form {
    #[default]
    Open,
    Closed,
    Periodic,
}

impl Form {
    pub fn index(self) -> i64 {
        match self {
            Form::Open => 0,
            Form::Closed => 1,
            Form::Periodic => 2,
        }
    }
    pub fn from_index(i: i64) -> Form {
        match i {
            1 => Form::Closed,
            2 => Form::Periodic,
            _ => Form::Open,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NurbsCurve {
    pub degree: usize,
    pub form: Form,
    pub cvs: Vec<DVec3>,
    /// `cvs.len() + degree - 1` values, as Maya writes them.
    pub knots: Vec<f64>,
}

/// Maya's default knots for an open curve: clamped ends, a unit apart.
pub fn open_knots(n_cvs: usize, degree: usize) -> Vec<f64> {
    let mut k = Vec::with_capacity(n_cvs + degree - 1);
    for _ in 0..degree {
        k.push(0.0);
    }
    let spans = n_cvs.saturating_sub(degree);
    for i in 1..spans {
        k.push(i as f64);
    }
    for _ in 0..degree {
        k.push(spans as f64);
    }
    k
}

/// Knots of a periodic curve of `spans` spans: uniform, running from `1 - degree`.
pub fn periodic_knots(spans: usize, degree: usize) -> Vec<f64> {
    let n = spans + degree + degree - 1;
    (0..n).map(|i| i as f64 + 1.0 - degree as f64).collect()
}

impl NurbsCurve {
    pub fn new(degree: usize, form: Form, cvs: Vec<DVec3>, knots: Vec<f64>) -> NurbsCurve {
        NurbsCurve { degree, form, cvs, knots }
    }

    /// An open curve through its control vertices with default knots. Too few CVs for the degree lowers the degree.
    pub fn open(degree: usize, cvs: Vec<DVec3>) -> NurbsCurve {
        let degree = degree.min(cvs.len().saturating_sub(1)).max(1);
        let knots = open_knots(cvs.len(), degree);
        NurbsCurve { degree, form: Form::Open, cvs, knots }
    }

    /// A periodic curve from one lap of CVs; the wrapped ones are added.
    pub fn periodic(degree: usize, lap: Vec<DVec3>) -> NurbsCurve {
        let degree = degree.max(1);
        let spans = lap.len();
        let mut cvs = lap;
        for i in 0..degree {
            cvs.push(cvs[i % spans]);
        }
        NurbsCurve { degree, form: Form::Periodic, cvs, knots: periodic_knots(spans, degree) }
    }

    pub fn is_valid(&self) -> bool {
        self.degree >= 1 && self.cvs.len() > self.degree && self.knots.len() == self.cvs.len() + self.degree - 1
    }

    pub fn spans(&self) -> usize {
        self.cvs.len().saturating_sub(self.degree)
    }

    /// (minValue, maxValue) of the parameter.
    pub fn domain(&self) -> (f64, f64) {
        if !self.is_valid() {
            return (0.0, 0.0);
        }
        (self.knots[self.degree - 1], self.knots[self.cvs.len() - 1])
    }

    /// One lap of CVs: the wrapped ones of a periodic curve left off.
    pub fn distinct_cvs(&self) -> &[DVec3] {
        if self.form == Form::Periodic {
            &self.cvs[..self.spans()]
        } else {
            &self.cvs
        }
    }

    fn ext(&self, i: usize) -> f64 {
        // The phantom knots at each end of Maya's knot vector don't affect evaluation in the domain.
        let m = self.knots.len();
        if i == 0 {
            self.knots[0]
        } else if i > m {
            self.knots[m - 1]
        } else {
            self.knots[i - 1]
        }
    }

    fn span_at(&self, u: f64) -> usize {
        // Largest k with ext(k) <= u < ext(k+1), within the domain.
        let n = self.cvs.len();
        let d = self.degree;
        let mut k = d;
        for i in d..n {
            if self.ext(i) <= u {
                k = i;
            }
            if self.ext(i + 1) > u && self.ext(i) <= u {
                k = i;
                break;
            }
        }
        // At the very end the last non-empty span applies.
        let mut k = k.min(n - 1);
        while k > d && self.ext(k + 1) <= self.ext(k) {
            k -= 1;
        }
        k
    }

    /// The point at a parameter (clamped to the domain), by de Boor.
    pub fn eval(&self, u: f64) -> DVec3 {
        if !self.is_valid() {
            return self.cvs.first().copied().unwrap_or(DVec3::ZERO);
        }
        let (lo, hi) = self.domain();
        let u = u.clamp(lo, hi);
        let d = self.degree;
        let k = self.span_at(u);
        let mut pts: Vec<DVec3> = (0..=d).map(|j| self.cvs[k - d + j]).collect();
        for r in 1..=d {
            for j in (r..=d).rev() {
                let i = k - d + j;
                let (a, b) = (self.ext(i), self.ext(i + d - r + 1));
                let den = b - a;
                let t = if den.abs() < 1e-12 { 0.0 } else { (u - a) / den };
                pts[j] = pts[j - 1].lerp(pts[j], t);
            }
        }
        pts[d]
    }

    /// The tangent (derivative with respect to the parameter).
    pub fn tangent(&self, u: f64) -> DVec3 {
        let (lo, hi) = self.domain();
        let h = (hi - lo).max(1e-6) * 1e-4;
        let (a, b) = ((u - h).max(lo), (u + h).min(hi));
        if b - a < 1e-12 {
            return DVec3::ZERO;
        }
        (self.eval(b) - self.eval(a)) / (b - a)
    }

    /// Distinct knots in the domain: the parameters of the edit points.
    pub fn distinct_params(&self) -> Vec<f64> {
        if !self.is_valid() {
            return Vec::new();
        }
        let (lo, hi) = self.domain();
        let mut v = Vec::new();
        for &k in &self.knots {
            if k >= lo - 1e-9 && k <= hi + 1e-9 && v.last().is_none_or(|l: &f64| (k - *l).abs() > 1e-9) {
                v.push(k);
            }
        }
        v
    }

    pub fn edit_points(&self) -> Vec<DVec3> {
        self.distinct_params().into_iter().map(|u| self.eval(u)).collect()
    }

    /// Points along the curve, `per_span` to a span and a straight run for degree 1.
    pub fn tessellate(&self, per_span: usize) -> Vec<DVec3> {
        if !self.is_valid() {
            return self.cvs.clone();
        }
        let params = self.distinct_params();
        let steps = if self.degree == 1 { 1 } else { per_span.max(1) };
        let mut out = Vec::with_capacity(params.len() * steps + 1);
        for w in params.windows(2) {
            for s in 0..steps {
                out.push(self.eval(w[0] + (w[1] - w[0]) * s as f64 / steps as f64));
            }
        }
        out.push(self.eval(*params.last().unwrap_or(&0.0)));
        out
    }

    /// Arc length.
    pub fn length(&self) -> f64 {
        let p = self.tessellate(32);
        p.windows(2).map(|w| (w[1] - w[0]).length()).sum()
    }

    /// The parameter a fraction of the way along the arc.
    pub fn param_at_fraction(&self, f: f64) -> f64 {
        let (lo, hi) = self.domain();
        let n = (self.spans() * 32).max(32);
        let mut acc = vec![0.0];
        let mut last = self.eval(lo);
        for i in 1..=n {
            let p = self.eval(lo + (hi - lo) * i as f64 / n as f64);
            acc.push(acc[i - 1] + (p - last).length());
            last = p;
        }
        let target = f.clamp(0.0, 1.0) * acc[n];
        let i = acc.partition_point(|&a| a < target).clamp(1, n);
        let seg = acc[i] - acc[i - 1];
        let t = if seg > 1e-12 { (target - acc[i - 1]) / seg } else { 0.0 };
        lo + (hi - lo) * (i as f64 - 1.0 + t) / n as f64
    }

    /// The parameter of the closest point on the curve.
    pub fn closest_param(&self, p: DVec3) -> f64 {
        let (lo, hi) = self.domain();
        let n = (self.spans() * 16).max(16);
        let mut best = (f64::MAX, lo);
        for i in 0..=n {
            let u = lo + (hi - lo) * i as f64 / n as f64;
            let d = (self.eval(u) - p).length_squared();
            if d < best.0 {
                best = (d, u);
            }
        }
        // Refine by golden-ish bisection around the sample.
        let step = (hi - lo) / n as f64;
        let (mut a, mut b) = ((best.1 - step).max(lo), (best.1 + step).min(hi));
        for _ in 0..40 {
            let (m1, m2) = (a + (b - a) / 3.0, b - (b - a) / 3.0);
            if (self.eval(m1) - p).length_squared() < (self.eval(m2) - p).length_squared() {
                b = m2;
            } else {
                a = m1;
            }
        }
        (a + b) * 0.5
    }

    pub fn transformed(&self, m: &glam::DMat4) -> NurbsCurve {
        let mut c = self.clone();
        for p in c.cvs.iter_mut() {
            *p = m.transform_point3(*p);
        }
        c
    }

    pub fn reversed(&self) -> NurbsCurve {
        let (lo, hi) = self.domain();
        let mut k: Vec<f64> = self.knots.iter().rev().map(|x| lo + hi - x).collect();
        if self.form == Form::Periodic {
            k = self.knots.clone();
        }
        let mut cvs: Vec<DVec3> = self.cvs.iter().rev().copied().collect();
        if self.form == Form::Periodic {
            let lap: Vec<DVec3> = self.distinct_cvs().iter().rev().copied().collect();
            cvs = lap.clone();
            for i in 0..self.degree {
                cvs.push(lap[i % lap.len()]);
            }
        }
        NurbsCurve { degree: self.degree, form: self.form, cvs, knots: k }
    }

    /// Boehm's knot insertion.
    pub fn insert_knot(&self, u: f64) -> NurbsCurve {
        if !self.is_valid() {
            return self.clone();
        }
        let d = self.degree;
        let k = self.span_at(u);
        let mut cvs = Vec::with_capacity(self.cvs.len() + 1);
        for i in 0..=self.cvs.len() {
            if i + d <= k {
                cvs.push(self.cvs[i]);
            } else if i > k {
                cvs.push(self.cvs[i - 1]);
            } else {
                let (a, b) = (self.ext(i), self.ext(i + d));
                let t = if (b - a).abs() < 1e-12 { 0.0 } else { (u - a) / (b - a) };
                cvs.push(self.cvs[i - 1].lerp(self.cvs[i], t));
            }
        }
        let mut knots = self.knots.clone();
        let at = knots.partition_point(|&x| x <= u);
        knots.insert(at, u);
        NurbsCurve { degree: d, form: self.form, cvs, knots }
    }

    /// Two open curves meeting at u.
    pub fn split(&self, u: f64) -> (NurbsCurve, NurbsCurve) {
        let (lo, hi) = self.domain();
        let u = u.clamp(lo, hi);
        // Sampling-free when the curve allows: raise u's multiplicity to the degree.
        let mut c = self.clone();
        let mult = c.knots.iter().filter(|x| (**x - u).abs() < 1e-9).count();
        for _ in mult..self.degree {
            c = c.insert_knot(u);
        }
        let d = self.degree;
        // ext index of the first u; the CV there is on the curve.
        let first = c.knots.iter().position(|x| (*x - u).abs() < 1e-9).unwrap_or(0);
        let split_cv = first + 1 - 1;
        let left_cvs: Vec<DVec3> = c.cvs[..=split_cv.min(c.cvs.len() - 1)].to_vec();
        let right_cvs: Vec<DVec3> = c.cvs[split_cv.min(c.cvs.len() - 1)..].to_vec();
        let mut left_knots: Vec<f64> = c.knots[..first + d].to_vec();
        left_knots.truncate(left_cvs.len() + d - 1);
        let mut right_knots: Vec<f64> = c.knots[first..].to_vec();
        right_knots.truncate(right_cvs.len() + d - 1);
        let fix = |cvs: Vec<DVec3>, knots: Vec<f64>| {
            if cvs.len() > d && knots.len() == cvs.len() + d - 1 {
                NurbsCurve { degree: d, form: Form::Open, cvs, knots }
            } else {
                NurbsCurve::open(d, cvs)
            }
        };
        (fix(left_cvs, left_knots), fix(right_cvs, right_knots))
    }

    /// The curve closed: periodic, its CVs one lap (Maya's closeCurve with preserve shape off).
    pub fn closed(&self) -> NurbsCurve {
        if self.form == Form::Periodic {
            return self.clone();
        }
        NurbsCurve::periodic(self.degree, self.cvs.clone())
    }

    /// The curve opened: clamped ends with the same CVs, one lap.
    pub fn opened(&self) -> NurbsCurve {
        if self.form != Form::Periodic {
            return self.clone();
        }
        NurbsCurve::open(self.degree, self.distinct_cvs().to_vec())
    }

    /// An open curve interpolating points, chord-length parameterised (global interpolation).
    pub fn through(degree: usize, pts: &[DVec3]) -> NurbsCurve {
        let n = pts.len();
        if n < 2 {
            return NurbsCurve::open(1, pts.to_vec());
        }
        let degree = degree.min(n - 1).max(1);
        if degree == 1 {
            return NurbsCurve::open(1, pts.to_vec());
        }
        let total: f64 = pts.windows(2).map(|w| (w[1] - w[0]).length()).sum::<f64>().max(1e-9);
        let mut u = vec![0.0; n];
        for i in 1..n {
            u[i] = u[i - 1] + (pts[i] - pts[i - 1]).length() / total;
        }
        u[n - 1] = 1.0;
        // Averaged knots: the full vector has n + degree + 1 values.
        let mut full = vec![0.0; n + degree + 1];
        for k in full.iter_mut().skip(n) {
            *k = 1.0;
        }
        for j in 1..n - degree {
            full[j + degree] = u[j..j + degree].iter().sum::<f64>() / degree as f64;
        }
        // Basis matrix, solved directly: curves here have few points.
        let mut a = vec![vec![0.0; n]; n];
        for (r, &ur) in u.iter().enumerate() {
            let b = basis_row(&full, degree, n, ur);
            a[r] = b;
        }
        let cvs = solve(&a, pts);
        let knots = full[1..full.len() - 1].to_vec();
        // Knots scaled to whole spans over the distinct range, as Maya's EP tool shows.
        let scale = (n - degree) as f64;
        NurbsCurve { degree, form: Form::Open, cvs, knots: knots.iter().map(|x| x * scale).collect() }
    }

    /// Rebuilt to uniform spans and a degree: points sampled by arc length, then interpolated.
    pub fn rebuilt(&self, spans: usize, degree: usize) -> NurbsCurve {
        let spans = spans.max(1);
        let degree = degree.max(1);
        if self.form == Form::Periodic {
            let lap: Vec<DVec3> = (0..spans).map(|i| self.eval(self.param_at_fraction(i as f64 / spans as f64))).collect();
            return NurbsCurve::periodic(degree, lap);
        }
        let n = spans + degree;
        let pts: Vec<DVec3> = (0..n).map(|i| self.eval(self.param_at_fraction(i as f64 / (n - 1) as f64))).collect();
        let c = NurbsCurve::through(degree, &pts);
        // Uniform knots with the fitted CVs' count.
        NurbsCurve::new(c.degree, Form::Open, c.cvs.clone(), open_knots(c.cvs.len(), c.degree))
    }

    /// Laplacian smoothing of the CVs; the ends stay on open curves.
    pub fn smoothed(&self, amount: f64, iterations: usize) -> NurbsCurve {
        let mut c = self.clone();
        let lap = c.distinct_cvs().len();
        let per = c.form == Form::Periodic;
        for _ in 0..iterations {
            let old = c.cvs.clone();
            for i in 0..lap {
                if !per && (i == 0 || i == lap - 1) {
                    continue;
                }
                let (a, b) = (old[(i + lap - 1) % lap], old[(i + 1) % lap]);
                c.cvs[i] = old[i].lerp((a + b) * 0.5, amount);
            }
            if per {
                for i in 0..c.degree {
                    c.cvs[lap + i] = c.cvs[i % lap];
                }
            }
        }
        c
    }

    /// This curve followed by another, joined with a straight connection between its end and the other's start.
    pub fn attached(&self, other: &NurbsCurve) -> NurbsCurve {
        let d = self.degree.max(other.degree);
        let mut pts = self.tessellate(8);
        let more = other.tessellate(8);
        if pts.last().zip(more.first()).is_some_and(|(a, b)| (*a - *b).length() < 1e-9) {
            pts.extend(more.into_iter().skip(1));
        } else {
            pts.extend(more);
        }
        let want = self.spans() + other.spans() + 1;
        NurbsCurve::open(d, pts).rebuilt(want, d)
    }

    /// Offset within the plane of the curve (or `normal`), by sampling and interpolating.
    pub fn offset(&self, distance: f64, normal: Option<DVec3>) -> NurbsCurve {
        let (lo, hi) = self.domain();
        let params: Vec<f64> = self.distinct_params();
        let n = normal.unwrap_or_else(|| self.plane_normal());
        let samples = (self.spans() * 4).max(8);
        let at = |u: f64| {
            let t = self.tangent(u).normalize_or_zero();
            self.eval(u) + t.cross(n).normalize_or_zero() * distance
        };
        let _ = params;
        let pts: Vec<DVec3> = (0..=samples).map(|i| at(lo + (hi - lo) * i as f64 / samples as f64)).collect();
        let c = NurbsCurve::through(self.degree, &pts);
        c.rebuilt(self.spans().max(1), self.degree)
    }

    /// The best plane's normal through the curve's CVs.
    pub fn plane_normal(&self) -> DVec3 {
        let c: DVec3 = self.cvs.iter().copied().sum::<DVec3>() / self.cvs.len().max(1) as f64;
        let mut n = DVec3::ZERO;
        for w in self.cvs.windows(2) {
            n += (w[0] - c).cross(w[1] - c);
        }
        if n.length() < 1e-9 {
            DVec3::Y
        } else {
            n.normalize()
        }
    }

    /// Lengthened straight along the end tangents by distance.
    pub fn extended(&self, distance: f64, at_start: bool, at_end: bool) -> NurbsCurve {
        let (lo, hi) = self.domain();
        let mut pts = self.tessellate(8);
        if at_start {
            let t = self.tangent(lo).normalize_or_zero();
            pts.insert(0, pts[0] - t * distance);
        }
        if at_end {
            let t = self.tangent(hi).normalize_or_zero();
            let last = *pts.last().unwrap();
            pts.push(last + t * distance);
        }
        NurbsCurve::through(self.degree, &pts).rebuilt(self.spans() + 1, self.degree)
    }
}

fn basis_row(full: &[f64], degree: usize, n: usize, u: f64) -> Vec<f64> {
    // Cox-de Boor over the full clamped knot vector.
    let mut b = vec![0.0; full.len() - 1];
    for i in 0..full.len() - 1 {
        if u >= full[i] && u < full[i + 1] {
            b[i] = 1.0;
        }
    }
    if u >= 1.0 {
        // The last basis function takes the end.
        for x in b.iter_mut() {
            *x = 0.0;
        }
        b[n - 1] = 1.0;
    }
    for d in 1..=degree {
        for i in 0..full.len() - 1 - d {
            let l = if (full[i + d] - full[i]).abs() < 1e-12 { 0.0 } else { (u - full[i]) / (full[i + d] - full[i]) * b[i] };
            let r = if (full[i + d + 1] - full[i + 1]).abs() < 1e-12 { 0.0 } else { (full[i + d + 1] - u) / (full[i + d + 1] - full[i + 1]) * b[i + 1] };
            b[i] = l + r;
        }
    }
    b.truncate(n);
    b.resize(n, 0.0);
    b
}

fn solve(a: &[Vec<f64>], rhs: &[DVec3]) -> Vec<DVec3> {
    let n = rhs.len();
    let mut m: Vec<Vec<f64>> = a.to_vec();
    let mut x: Vec<DVec3> = rhs.to_vec();
    for c in 0..n {
        let mut p = c;
        for r in c + 1..n {
            if m[r][c].abs() > m[p][c].abs() {
                p = r;
            }
        }
        m.swap(c, p);
        x.swap(c, p);
        let d = m[c][c];
        if d.abs() < 1e-14 {
            continue;
        }
        for r in c + 1..n {
            let f = m[r][c] / d;
            if f == 0.0 {
                continue;
            }
            for k in c..n {
                let v = m[c][k];
                m[r][k] -= f * v;
            }
            let v = x[c];
            x[r] -= v * f;
        }
    }
    for c in (0..n).rev() {
        let mut v = x[c];
        for k in c + 1..n {
            v -= x[k] * m[c][k];
        }
        x[c] = if m[c][c].abs() < 1e-14 { v } else { v / m[c][c] };
    }
    x
}

/// Maya's `circle`: a periodic curve whose points hit the radius at the section ends. Sections are counted from the first
/// point axis around the normal; with sweep below 360 the curve is an open arc.
pub fn circle(center: DVec3, normal: DVec3, first: DVec3, radius: f64, degree: usize, sections: usize, sweep_deg: f64) -> NurbsCurve {
    let n = normal.normalize_or_zero();
    let n = if n == DVec3::ZERO { DVec3::Z } else { n };
    let mut x = first - n * first.dot(n);
    if x.length() < 1e-9 {
        x = n.any_orthonormal_vector();
    }
    let x = x.normalize();
    let y = n.cross(x);
    let sections = sections.max(3);
    let step = sweep_deg.to_radians() / sections as f64;
    if (sweep_deg - 360.0).abs() < 1e-6 {
        // For a uniform B-spline a point sits at (P- + 4P + P+)/6, so the CVs lie a little outside the radius.
        let r = if degree >= 3 { radius * 6.0 / (4.0 + 2.0 * step.cos()) } else if degree == 2 { radius * 2.0 / (1.0 + step.cos()) } else { radius };
        let lap: Vec<DVec3> = (0..sections).map(|i| center + (x * (step * (i + 1) as f64).cos() + y * (step * (i + 1) as f64).sin()) * r).collect();
        return NurbsCurve::periodic(degree, lap);
    }
    let pts: Vec<DVec3> = (0..=sections).map(|i| center + (x * (step * i as f64).cos() + y * (step * i as f64).sin()) * radius).collect();
    NurbsCurve::through(degree, &pts).rebuilt(sections, degree)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_curves_hit_their_ends() {
        let c = NurbsCurve::open(3, vec![DVec3::ZERO, DVec3::X, DVec3::new(2.0, 1.0, 0.0), DVec3::new(3.0, 1.0, 0.0), DVec3::new(4.0, 0.0, 0.0)]);
        assert_eq!(c.knots, vec![0.0, 0.0, 0.0, 1.0, 2.0, 2.0, 2.0]);
        assert_eq!(c.domain(), (0.0, 2.0));
        assert!((c.eval(0.0) - DVec3::ZERO).length() < 1e-9);
        assert!((c.eval(2.0) - DVec3::new(4.0, 0.0, 0.0)).length() < 1e-9);
        assert_eq!(c.spans(), 2);
    }

    #[test]
    fn circle_matches_maya_cvs() {
        let c = circle(DVec3::ZERO, DVec3::Y, DVec3::X, 1.0, 3, 8, 360.0);
        assert_eq!(c.cvs.len(), 11);
        assert_eq!(c.form, Form::Periodic);
        // Maya's first CVs for a unit circle about Y.
        assert!((c.cvs[0] - DVec3::new(0.783612, 0.0, -0.783612)).length() < 1e-5, "{:?}", c.cvs[0]);
        assert!((c.cvs[1] - DVec3::new(0.0, 0.0, -1.108194)).length() < 1e-5, "{:?}", c.cvs[1]);
        for i in 0..=80 {
            let (lo, hi) = c.domain();
            let p = c.eval(lo + (hi - lo) * i as f64 / 80.0);
            assert!((p.length() - 1.0).abs() < 0.01, "{p:?}");
        }
    }

    #[test]
    fn knot_insertion_keeps_the_shape() {
        let c = NurbsCurve::open(3, vec![DVec3::ZERO, DVec3::X, DVec3::new(2.0, 1.0, 0.0), DVec3::new(3.0, 1.0, 0.0), DVec3::new(4.0, 0.0, 0.0)]);
        let d = c.insert_knot(0.5);
        assert_eq!(d.cvs.len(), 6);
        for i in 0..=20 {
            let u = 2.0 * i as f64 / 20.0;
            assert!((c.eval(u) - d.eval(u)).length() < 1e-9);
        }
    }

    #[test]
    fn split_and_interpolate() {
        let c = NurbsCurve::open(3, vec![DVec3::ZERO, DVec3::X, DVec3::new(2.0, 1.0, 0.0), DVec3::new(3.0, 1.0, 0.0), DVec3::new(4.0, 0.0, 0.0)]);
        let (a, b) = c.split(1.0);
        assert!((a.eval(a.domain().1) - c.eval(1.0)).length() < 1e-6, "{:?} {:?}", a, c.eval(1.0));
        assert!((b.eval(b.domain().0) - c.eval(1.0)).length() < 1e-6);
        assert!((b.eval(b.domain().1) - c.eval(2.0)).length() < 1e-6);
        let pts = [DVec3::ZERO, DVec3::new(1.0, 1.0, 0.0), DVec3::new(2.0, 0.0, 0.0), DVec3::new(3.0, 1.0, 0.0)];
        let t = NurbsCurve::through(3, &pts);
        for p in pts {
            let u = t.closest_param(p);
            assert!((t.eval(u) - p).length() < 1e-4, "{p:?} {u}");
        }
    }

    #[test]
    fn reverse_and_length() {
        let c = NurbsCurve::open(1, vec![DVec3::ZERO, DVec3::X, DVec3::new(1.0, 1.0, 0.0)]);
        assert!((c.length() - 2.0).abs() < 1e-9);
        let r = c.reversed();
        assert!((r.eval(r.domain().0) - DVec3::new(1.0, 1.0, 0.0)).length() < 1e-9);
        assert!((c.eval(c.param_at_fraction(0.5)) - DVec3::X).length() < 1e-6);
    }
}
