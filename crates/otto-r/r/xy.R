# XY analyses: correlation, linear regression and nonlinear regression (curve fitting)
# with Prism's equations and parameter names.

xy_data <- function(data) {
  x <- num(data$x)
  y <- num(data$y)
  if (length(x) != length(y)) stop("X and Y need the same number of values")
  list(x = x, y = y)
}

curve_x <- function(o, x) {
  r <- num(o$xrange)
  if (length(r) != 2 || !all(is.finite(r)) || r[1] >= r[2]) r <- range(x)
  seq(r[1], r[2], length.out = 200)
}

curve_points <- function(xs, ys) {
  ok <- is.finite(xs) & is.finite(ys)
  list(x = arr(xs[ok]), y = arr(ys[ok]))
}

op_correlation <- function(data, o) {
  d <- xy_data(data)
  n <- length(d$x)
  if (n < 3) stop("correlation needs at least three XY pairs")
  conf <- o$confidence %||% 0.95
  alternative <- if (isTRUE(o$one_tailed)) {
    if (stats::cor(d$x, d$y, method = o$method %||% "pearson") >= 0) "greater" else "less"
  } else "two.sided"
  if (identical(o$method, "spearman")) {
    ct <- suppressWarnings(stats::cor.test(d$x, d$y, method = "spearman", alternative = alternative, exact = n < 1290 && !any(duplicated(d$x)) && !any(duplicated(d$y))))
    list(method = "spearman", r = unname(ct$estimate), ci = NULL, p = ct$p.value, n = n,
      exact = isTRUE(n < 1290 && !any(duplicated(d$x)) && !any(duplicated(d$y))))
  } else {
    ct <- stats::cor.test(d$x, d$y, method = "pearson", alternative = alternative, conf.level = conf)
    r <- unname(ct$estimate)
    # Two-sided interval even for a one-tailed P value, as Prism reports.
    ci <- if (n > 3) stats::cor.test(d$x, d$y, conf.level = conf)$conf.int else c(NA, NA)
    list(method = "pearson", r = r, ci = arr(ci), r_squared = r^2, t = unname(ct$statistic), df = unname(ct$parameter), p = ct$p.value, n = n)
  }
}

op_linear_regression <- function(data, o) {
  d <- xy_data(data)
  n <- length(d$x)
  if (n < 3) stop("linear regression needs at least three points")
  if (length(unique(d$x)) < 2) stop("linear regression needs at least two different X values")
  conf <- o$confidence %||% 0.95
  fit <- stats::lm(d$y ~ d$x)
  s <- summary(fit)
  co <- s$coefficients
  ci <- stats::confint(fit, level = conf)
  fs <- s$fstatistic
  slope <- co[2, 1]
  intercept <- co[1, 1]
  xs <- curve_x(o, d$x)
  list(
    slope = list(value = slope, se = co[2, 2], ci = arr(ci[2, ])),
    intercept = list(value = intercept, se = co[1, 2], ci = arr(ci[1, ])),
    x_intercept = -intercept / slope,
    inv_slope = 1 / slope,
    r_squared = s$r.squared,
    syx = s$sigma,
    f = unname(fs[1]), dfn = unname(fs[2]), dfd = unname(fs[3]),
    p = stats::pf(fs[1], fs[2], fs[3], lower.tail = FALSE)[[1]],
    n = n,
    ss = sum(stats::residuals(fit)^2),
    curve = curve_points(xs, intercept + slope * xs)
  )
}

# --- Curve fitting ---------------------------------------------------------------------

dr <- function(x, bottom, top, logec50, hill) bottom + (top - bottom) / (1 + 10^((logec50 - x) * hill))

dr5 <- function(x, bottom, top, logec50, hill, s) {
  logxb <- logec50 + (1 / hill) * log10(2^(1 / s) - 1)
  bottom + (top - bottom) / (1 + 10^((logxb - x) * hill))^s
}

# Rough starting values; every model tries several.
mid_x <- function(x, y) {
  half <- (min(y) + max(y)) / 2
  x[which.min(abs(y - half))]
}

trend <- function(x, y) {
  r <- suppressWarnings(stats::cor(x, y))
  if (is.finite(r) && r < 0) -1 else 1
}

rate_guess <- function(x) 3 / max(diff(range(x)), 1e-12)

starts_dr <- function(x, y, params) {
  b <- min(y)
  t <- max(y)
  sgn <- trend(x, y)
  out <- list()
  for (lx in unique(c(mid_x(x, y), stats::median(x), mean(range(x))))) {
    for (h in c(1, 0.5, 2)) {
      out[[length(out) + 1]] <- list(Bottom = b, Top = t, LogEC50 = lx, LogIC50 = lx, HillSlope = sgn * h, S = 1)
    }
  }
  out
}

starts_conc <- function(x, y, increasing) {
  b <- min(y)
  t <- max(y)
  mx <- max(mid_x(x, y), min(x[x > 0], 1e-6))
  lapply(c(1, 0.5, 2), function(h) list(Bottom = if (increasing) b else b, Top = t, EC50 = mx, IC50 = mx, HillSlope = h))
}

starts_decay <- function(x, y) {
  y0 <- mean(y[x == min(x)])
  plateau <- mean(y[x == max(x)])
  k <- rate_guess(x)
  lapply(c(1, 0.1, 10), function(f) list(Y0 = y0, Plateau = plateau, K = k * f, X0 = min(x)))
}

starts_growth <- function(x, y) {
  pos <- y > 0
  k <- rate_guess(x) / 3
  y0 <- mean(y[x == min(x)])
  if (sum(pos) >= 2) {
    co <- stats::coef(stats::lm(log(y[pos]) ~ x[pos]))
    k <- co[[2]]
    y0 <- exp(co[[1]])
  }
  lapply(c(1, 0.5, 2), function(f) list(Y0 = y0, K = k * f))
}

starts_binding <- function(x, y) {
  bmax <- max(y)
  kd <- max(mid_x(x, y), 1e-9)
  lapply(c(1, 0.3, 3), function(f) list(Bmax = bmax, Vmax = bmax, Kd = kd * f, Km = kd * f, h = 1, NS = 0, Background = min(0, min(y)), Ki = max(x) * 10))
}

starts_linear <- function(x, y, degree) {
  co <- stats::coef(stats::lm(y ~ stats::poly(x, degree, raw = TRUE)))
  co[!is.finite(co)] <- 0
  names(co) <- paste0("B", 0:degree)
  st <- as.list(co)
  st$YIntercept <- co[[1]]
  st$Slope <- if (degree >= 1) co[[2]] else 0
  list(st)
}

fit_models <- list(
  line = list(params = c("YIntercept", "Slope"), f = function(x, p) p$YIntercept + p$Slope * x, start = function(x, y) starts_linear(x, y, 1)),
  poly2 = list(params = c("B0", "B1", "B2"), f = function(x, p) p$B0 + p$B1 * x + p$B2 * x^2, start = function(x, y) starts_linear(x, y, 2)),
  poly3 = list(params = c("B0", "B1", "B2", "B3"), f = function(x, p) p$B0 + p$B1 * x + p$B2 * x^2 + p$B3 * x^3, start = function(x, y) starts_linear(x, y, 3)),
  agonist_1pl = list(params = "LogEC50", f = function(x, p) 100 / (1 + 10^(p$LogEC50 - x)), start = starts_dr),
  agonist_2pl = list(params = c("LogEC50", "HillSlope"), f = function(x, p) 100 / (1 + 10^((p$LogEC50 - x) * p$HillSlope)), start = starts_dr),
  agonist_3pl = list(params = c("Bottom", "Top", "LogEC50"), f = function(x, p) dr(x, p$Bottom, p$Top, p$LogEC50, 1), start = starts_dr),
  agonist_4pl = list(params = c("Bottom", "Top", "LogEC50", "HillSlope"), f = function(x, p) dr(x, p$Bottom, p$Top, p$LogEC50, p$HillSlope), start = starts_dr),
  agonist_5pl = list(params = c("Bottom", "Top", "LogEC50", "HillSlope", "S"), f = function(x, p) dr5(x, p$Bottom, p$Top, p$LogEC50, p$HillSlope, p$S), start = starts_dr, refine = "agonist_4pl"),
  agonist_conc = list(params = c("Bottom", "Top", "EC50", "HillSlope"), f = function(x, p) p$Bottom + (p$Top - p$Bottom) * x^p$HillSlope / (p$EC50^p$HillSlope + x^p$HillSlope), start = function(x, y) starts_conc(x, y, TRUE)),
  inhibitor_1pl = list(params = "LogIC50", f = function(x, p) 100 / (1 + 10^(x - p$LogIC50)), start = starts_dr),
  inhibitor_2pl = list(params = c("LogIC50", "HillSlope"), f = function(x, p) 100 / (1 + 10^((p$LogIC50 - x) * p$HillSlope)), start = starts_dr),
  inhibitor_3pl = list(params = c("Bottom", "Top", "LogIC50"), f = function(x, p) dr(x, p$Bottom, p$Top, p$LogIC50, -1), start = starts_dr),
  inhibitor_4pl = list(params = c("Bottom", "Top", "LogIC50", "HillSlope"), f = function(x, p) dr(x, p$Bottom, p$Top, p$LogIC50, p$HillSlope), start = starts_dr),
  inhibitor_5pl = list(params = c("Bottom", "Top", "LogIC50", "HillSlope", "S"), f = function(x, p) dr5(x, p$Bottom, p$Top, p$LogIC50, p$HillSlope, p$S), start = starts_dr, refine = "inhibitor_4pl"),
  inhibitor_conc = list(params = c("Bottom", "Top", "IC50", "HillSlope"), f = function(x, p) p$Bottom + (p$Top - p$Bottom) / (1 + (x / p$IC50)^p$HillSlope), start = function(x, y) starts_conc(x, y, FALSE)),
  one_phase_decay = list(params = c("Y0", "Plateau", "K"), f = function(x, p) (p$Y0 - p$Plateau) * exp(-p$K * x) + p$Plateau, start = starts_decay),
  plateau_decay = list(params = c("X0", "Y0", "Plateau", "K"), f = function(x, p) ifelse(x < p$X0, p$Y0, p$Plateau + (p$Y0 - p$Plateau) * exp(-p$K * (x - p$X0))), start = starts_decay),
  two_phase_decay = list(params = c("Y0", "Plateau", "PercentFast", "KFast", "KSlow"),
    f = function(x, p) {
      span <- p$Y0 - p$Plateau
      p$Plateau + span * p$PercentFast * 0.01 * exp(-p$KFast * x) + span * (100 - p$PercentFast) * 0.01 * exp(-p$KSlow * x)
    },
    start = function(x, y) lapply(starts_decay(x, y), function(s) list(Y0 = s$Y0, Plateau = s$Plateau, PercentFast = 50, KFast = s$K * 3, KSlow = s$K / 3))),
  one_phase_association = list(params = c("Y0", "Plateau", "K"), f = function(x, p) p$Y0 + (p$Plateau - p$Y0) * (1 - exp(-p$K * x)), start = starts_decay),
  exponential_growth = list(params = c("Y0", "K"), f = function(x, p) p$Y0 * exp(p$K * x), start = starts_growth),
  one_site_specific = list(params = c("Bmax", "Kd"), f = function(x, p) p$Bmax * x / (p$Kd + x), start = starts_binding),
  one_site_total = list(params = c("Bmax", "Kd", "NS", "Background"), f = function(x, p) p$Bmax * x / (p$Kd + x) + p$NS * x + p$Background, start = starts_binding),
  specific_hill = list(params = c("Bmax", "Kd", "h"), f = function(x, p) p$Bmax * x^p$h / (p$Kd^p$h + x^p$h), start = starts_binding),
  michaelis_menten = list(params = c("Vmax", "Km"), f = function(x, p) p$Vmax * x / (p$Km + x), start = starts_binding),
  substrate_inhibition = list(params = c("Vmax", "Km", "Ki"), f = function(x, p) p$Vmax * x / (p$Km + x * (1 + x / p$Ki)), start = starts_binding),
  gaussian = list(params = c("Amplitude", "Mean", "SD"), f = function(x, p) p$Amplitude * exp(-0.5 * ((x - p$Mean) / p$SD)^2),
    start = function(x, y) lapply(c(1, 0.5, 2), function(f) list(Amplitude = max(y), Mean = x[which.max(y)], SD = f * diff(range(x)) / 4)))
)

op_models <- function() lapply(fit_models, function(m) list(params = I(m$params)))

# Least-squares fit of one model by Levenberg-Marquardt (minpack.lm), from several starts.
fit_once <- function(model, x, y, fixed, starts) {
  use_pkg("minpack.lm")
  free <- setdiff(model$params, names(fixed))
  best <- NULL
  for (st in starts) {
    par <- unlist(st[free])
    if (length(par) != length(free) || !all(is.finite(par))) next
    resid <- function(p) {
      # Trial parameters outside a model's domain give NaN (e.g. 5PL with S < 0): rejected below.
      r <- y - suppressWarnings(model$f(x, c(as.list(p), fixed)))
      r[!is.finite(r)] <- 1e100
      r
    }
    fit <- tryCatch(
      minpack.lm::nls.lm(par = par, fn = resid, control = minpack.lm::nls.lm.control(maxiter = 1000, ftol = 1e-12, ptol = 1e-12)),
      error = function(e) NULL
    )
    if (is.null(fit) || !is.finite(fit$deviance)) next
    if (is.null(best) || fit$deviance < best$deviance) best <- fit
  }
  best
}

op_curve_fit <- function(data, o) {
  d <- xy_data(data)
  ok <- is.finite(d$x) & is.finite(d$y)
  x <- d$x[ok]
  y <- d$y[ok]
  model <- fit_models[[o$model]]
  if (is.null(model)) stop("unknown model '", o$model, "'")
  fixed <- o$fixed %||% list()
  fixed <- fixed[names(fixed) %in% model$params]
  fixed <- lapply(fixed, as.numeric)
  free <- setdiff(model$params, names(fixed))
  n <- length(x)
  if (length(free) == 0) stop("every parameter is constrained to a constant; nothing to fit")
  if (n <= length(free)) stop(sprintf("%d points are too few to fit %d parameters", n, length(free)))
  conf <- o$confidence %||% 0.95
  starts <- model$start(x, y)
  if (!is.null(model$refine)) {
    base <- fit_once(fit_models[[model$refine]], x, y, fixed[names(fixed) %in% fit_models[[model$refine]]$params], fit_models[[model$refine]]$start(x, y))
    if (!is.null(base)) starts <- c(lapply(c(1, 0.5, 2), function(s) c(as.list(base$par), list(S = s))), starts)
  }
  starts <- lapply(starts, function(s) modifyList(s, fixed))
  fit <- fit_once(model, x, y, fixed, starts)
  if (is.null(fit)) stop("the fit did not converge; try constraining a parameter (Constraints tab)")
  par <- fit$par
  names(par) <- free
  df <- n - length(free)
  ss <- sum(fit$fvec^2)
  cov <- tryCatch(chol2inv(chol(fit$hessian)) * ss / df, error = function(e) NULL)
  if (!is.null(cov)) dimnames(cov) <- list(free, free)
  se <- if (is.null(cov)) rep(NA_real_, length(free)) else sqrt(pmax(diag(cov), 0))
  names(se) <- free
  tq <- stats::qt(1 - (1 - conf) / 2, df)
  params <- lapply(model$params, function(nm) {
    if (nm %in% free) {
      v <- par[[nm]]
      list(name = nm, value = v, se = se[[nm]], ci = arr(c(v - tq * se[[nm]], v + tq * se[[nm]])), fixed = FALSE)
    } else {
      list(name = nm, value = fixed[[nm]], se = NULL, ci = NULL, fixed = TRUE)
    }
  })
  values <- c(as.list(par), fixed)
  xs <- curve_x(o, x)
  list(
    model = o$model,
    params = params,
    derived = derived_values(o$model, values, free, se, cov, tq),
    r_squared = 1 - ss / sum((y - mean(y))^2),
    syx = sqrt(ss / df),
    ss = ss,
    df = df,
    n = n,
    converged = fit$info %in% 1:4,
    message = fit$message,
    curve = curve_points(xs, model$f(xs, values))
  )
}

# Values derived from the parameters, with confidence intervals transformed from the
# parameter's interval (EC50 from LogEC50, half-life from K) or from the covariance (Span).
derived_values <- function(model, v, free, se, cov, tq) {
  out <- list()
  add <- function(name, value, ci = NULL) out[[length(out) + 1]] <<- list(name = name, value = value, ci = if (is.null(ci)) NULL else arr(sort(ci)))
  ci_of <- function(nm) if (nm %in% free && is.finite(se[[nm]])) c(v[[nm]] - tq * se[[nm]], v[[nm]] + tq * se[[nm]]) else NULL
  for (nm in c("LogEC50", "LogIC50")) {
    if (!is.null(v[[nm]])) {
      ci <- ci_of(nm)
      add(sub("Log", "", nm), 10^v[[nm]], if (is.null(ci)) NULL else 10^ci)
    }
  }
  span <- function(hi, lo) {
    value <- v[[hi]] - v[[lo]]
    var <- 0
    if (!is.null(cov)) {
      if (hi %in% free) var <- var + cov[hi, hi]
      if (lo %in% free) var <- var + cov[lo, lo]
      if (hi %in% free && lo %in% free) var <- var - 2 * cov[hi, lo]
    }
    add("Span", value, if (is.null(cov) || var <= 0) NULL else c(value - tq * sqrt(var), value + tq * sqrt(var)))
  }
  if (all(c("Top", "Bottom") %in% names(v))) span("Top", "Bottom")
  rate <- function(k, half_name, tau = TRUE) {
    ci <- ci_of(k)
    pos <- !is.null(ci) && all(ci > 0)
    add(half_name, log(2) / abs(v[[k]]), if (pos) log(2) / ci else NULL)
    if (tau) add(if (k == "K") "Tau" else paste0("Tau (", k, ")"), 1 / abs(v[[k]]), if (pos) 1 / ci else NULL)
  }
  if (model %in% c("one_phase_decay", "plateau_decay")) {
    rate("K", "Half-life")
    span("Y0", "Plateau")
  }
  if (model == "one_phase_association") {
    rate("K", "Half-time")
    span("Plateau", "Y0")
  }
  if (model == "two_phase_decay") {
    rate("KFast", "Half-life (fast)", FALSE)
    rate("KSlow", "Half-life (slow)", FALSE)
  }
  if (model == "exponential_growth") rate("K", "Doubling time")
  if (model == "line") {
    add("X intercept", -v$YIntercept / v$Slope)
    add("1/slope", 1 / v$Slope)
  }
  out
}

# --- Area under the curve ----------------------------------------------------------------

# Area under the curve by the trapezoid rule on the mean Y at each X, measured from a
# baseline, with its standard error from the replicates (Gagnon & Peterson 1998,
# J Pharmacokinet Biopharm 26:87) and the peaks: regions between baseline crossings,
# ignoring those lower than a percentage of the Y range.
op_auc <- function(data, o) {
  d <- xy_data(data)
  ok <- is.finite(d$x) & is.finite(d$y)
  x <- d$x[ok]
  y <- d$y[ok]
  xs <- sort(unique(x))
  if (length(xs) < 2) stop("area under the curve needs at least two different X values")
  m <- vapply(xs, function(v) mean(y[x == v]), 0)
  n <- vapply(xs, function(v) sum(x == v), 0)
  s <- vapply(xs, function(v) if (sum(x == v) > 1) stats::sd(y[x == v]) else NA_real_, 0)
  baseline <- o$baseline %||% 0
  conf <- o$confidence %||% 0.95
  h <- m - baseline
  k <- length(xs)
  # Trapezoid weights: the area is sum(w * h).
  w <- c(xs[2] - xs[1], xs[-(1:2)] - xs[seq_len(k - 2)], xs[k] - xs[k - 1]) / 2
  total <- sum(w * h)
  df <- sum(n) - k
  se <- if (all(n > 1)) sqrt(sum(w^2 * s^2 / n)) else NA_real_
  ci <- if (is.finite(se) && df > 0) total + c(-1, 1) * stats::qt(1 - (1 - conf) / 2, df) * se else c(NA, NA)
  # Regions between baseline crossings (linear interpolation).
  regions <- list()
  cur <- NULL
  close_region <- function() {
    if (!is.null(cur) && cur$sign != 0) regions[[length(regions) + 1]] <<- cur
    cur <<- NULL
  }
  extend <- function(x0, h0, x1, h1) {
    sg <- sign(h0 + h1)
    if (is.null(cur) || cur$sign != sg) {
      close_region()
      cur <<- list(sign = sg, start = x0, end = x1, area = 0, peak_x = x0, peak_h = 0)
    }
    cur$end <<- x1
    cur$area <<- cur$area + (x1 - x0) * (h0 + h1) / 2
    for (p in list(c(x0, h0), c(x1, h1))) if (abs(p[2]) > abs(cur$peak_h)) {
      cur$peak_x <<- p[1]
      cur$peak_h <<- p[2]
    }
  }
  for (i in seq_len(k - 1)) {
    h0 <- h[i]; h1 <- h[i + 1]
    if (h0 * h1 < 0) {
      xc <- xs[i] + (xs[i + 1] - xs[i]) * h0 / (h0 - h1)
      extend(xs[i], h0, xc, 0)
      extend(xc, 0, xs[i + 1], h1)
    } else if (h0 == 0 && h1 == 0) {
      close_region()
    } else {
      extend(xs[i], h0, xs[i + 1], h1)
    }
  }
  close_region()
  threshold <- (o$min_peak_percent %||% 10) / 100 * (max(m) - min(m))
  peaks <- Filter(function(r) abs(r$peak_h) >= threshold && (r$sign > 0 || isTRUE(o$peaks_below)), regions)
  peak_total <- sum(vapply(peaks, function(r) r$area, 0))
  list(
    baseline = baseline, total_area = total, se = se, ci = arr(ci), df = df,
    n_x = k, n_values = sum(n), total_peak_area = peak_total,
    peaks = lapply(peaks, function(r) list(start = r$start, end = r$end, peak_x = r$peak_x, peak_y = r$peak_h + baseline,
      area = r$area, percent = if (peak_total != 0) 100 * r$area / peak_total else NA))
  )
}
