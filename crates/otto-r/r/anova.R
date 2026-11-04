# One-way ANOVA (ordinary, Brown-Forsythe and Welch, repeated measures, Kruskal-Wallis,
# Friedman) and the multiple comparisons that follow it, as in Prism.

op_anova <- function(data, o) {
  lognormal <- isTRUE(o$log)
  transform <- function(v, name) if (lognormal) log10_positive(v, sprintf("\"%s\"", name)) else v
  labels <- as.character(unlist(data$labels %||% list()))
  if (!is.null(data$groups)) {
    groups <- lapply(data$groups, num)
    if (length(labels) != length(groups)) labels <- paste("data set", seq_along(groups))
    groups <- mapply(transform, groups, labels, SIMPLIFY = FALSE)
  } else {
    rows <- lapply(data$rows, num)
    m <- do.call(rbind, rows)
    if (lognormal) {
      if (any(m <= 0)) stop("a lognormal ANOVA needs positive values")
      m <- log10(m)
    }
  }
  res <- switch(o$design,
    ordinary = anova_ordinary(groups),
    welch = anova_welch(groups),
    kruskal = anova_kruskal(groups),
    rm = anova_rm(m),
    friedman = anova_friedman(m),
    stop("unknown ANOVA design '", o$design, "'")
  )
  scenario <- attr(res, "scenario")
  attr(res, "scenario") <- NULL
  pairs <- lapply(data$pairs %||% list(), function(p) as.integer(unlist(p)) + 1L)
  if (length(pairs) > 0) {
    res$comparisons <- multiple_comparisons(pairs, o$method, scenario, o$alpha %||% 0.05, isTRUE(o$sphericity))
  } else {
    res$comparisons <- list()
  }
  tests <- num_tests(o$normality)
  if (length(tests) > 0 && o$design %in% c("ordinary", "welch", "rm")) {
    res$normality <- normality_tests(scenario$residuals, tests)
  }
  res
}

check_groups <- function(groups, min_n) {
  if (length(groups) < 2) stop("one-way ANOVA needs at least two data sets")
  small <- which(lengths(groups) < min_n)
  if (length(small) > 0) stop(sprintf("every data set needs at least %d values (one has %d)", min_n, lengths(groups)[small[1]]))
}

long_form <- function(groups) {
  list(y = unlist(groups), g = factor(rep(seq_along(groups), lengths(groups))))
}

f_result <- function(f, df1, df2, p) list(f = f, df1 = df1, df2 = df2, p = p)

anova_row <- function(source, ss, df, ms = NULL, f = NULL, p = NULL) list(source = source, ss = ss, df = df, ms = ms, f = f, p = p)

anova_ordinary <- function(groups) {
  check_groups(groups, 1)
  lf <- long_form(groups)
  k <- length(groups)
  n <- length(lf$y)
  if (n - k < 1) stop("one-way ANOVA needs more values than data sets")
  fit <- stats::aov(lf$y ~ lf$g)
  tab <- summary(fit)[[1]]
  ssb <- tab[1, "Sum Sq"]
  ssw <- tab[2, "Sum Sq"]
  dfb <- tab[1, "Df"]
  dfw <- tab[2, "Df"]
  msb <- tab[1, "Mean Sq"]
  msw <- tab[2, "Mean Sq"]
  f <- tab[1, "F value"]
  p <- tab[1, "Pr(>F)"]
  sst <- ssb + ssw
  all_two <- all(lengths(groups) >= 2)
  bf_sd <- NULL
  bartlett <- NULL
  if (all_two) {
    bf_sd <- brown_forsythe_sd(lf$y, lf$g)
    if (all(vapply(groups, stats::var, 0) > 0)) {
      bt <- stats::bartlett.test(groups)
      bartlett <- list(statistic = unname(bt$statistic), df = unname(bt$parameter), p = bt$p.value)
    }
  }
  res <- list(
    f = f_result(f, dfb, dfw, p),
    r_squared = ssb / sst,
    table = list(
      anova_row("Treatment (between columns)", ssb, dfb, msb, f, p),
      anova_row("Residual (within columns)", ssw, dfw, msw),
      anova_row("Total", sst, n - 1)
    ),
    mse = msw, df_error = dfw,
    brown_forsythe_sd = bf_sd, bartlett = bartlett,
    eta_squared = ssb / sst,
    omega_squared = max((ssb - dfb * msw) / (sst + msw), 0),
    aicc = aicc_compare(sst, 2, ssw, k + 1, n)
  )
  attr(res, "scenario") <- list(type = "pooled", groups = groups, mse = msw, df = dfw, residuals = unname(stats::residuals(fit)))
  res
}

# Brown-Forsythe test for equal SDs: Levene's test on deviations from the medians.
brown_forsythe_sd <- function(y, g) {
  use_pkg("car")
  lt <- suppressWarnings(car::leveneTest(y, g, center = stats::median))
  f <- lt[1, "F value"]
  if (!is.finite(f)) return(NULL)
  f_result(f, lt[1, "Df"], lt[2, "Df"], lt[1, "Pr(>F)"])
}

anova_welch <- function(groups) {
  check_groups(groups, 2)
  lf <- long_form(groups)
  w <- stats::oneway.test(lf$y ~ lf$g, var.equal = FALSE)
  # Brown-Forsythe F* test for equal means (Brown & Forsythe 1974).
  k <- length(groups)
  n <- length(lf$y)
  ni <- lengths(groups)
  means <- vapply(groups, mean, 0)
  vars <- vapply(groups, stats::var, 0)
  grand <- mean(lf$y)
  c <- (1 - ni / n) * vars
  f_bf <- sum(ni * (means - grand)^2) / sum(c)
  df2_bf <- sum(c)^2 / sum(c^2 / (ni - 1))
  res <- list(
    brown_forsythe = f_result(f_bf, k - 1, df2_bf, stats::pf(f_bf, k - 1, df2_bf, lower.tail = FALSE)),
    welch = f_result(unname(w$statistic), unname(w$parameter[1]), unname(w$parameter[2]), w$p.value)
  )
  attr(res, "scenario") <- list(type = "welch", groups = groups, residuals = unname(lf$y - means[lf$g]))
  res
}

anova_kruskal <- function(groups) {
  check_groups(groups, 1)
  lf <- long_form(groups)
  kt <- stats::kruskal.test(groups)
  r <- rank(lf$y)
  sums <- as.numeric(tapply(r, lf$g, sum))
  n <- length(lf$y)
  h <- unname(kt$statistic)
  res <- list(h = h, df = unname(kt$parameter), p = kt$p.value,
    mean_ranks = arr(sums / lengths(groups)), n_total = n, rank_sums = arr(sums),
    epsilon_squared = h * (n + 1) / (n^2 - 1))
  attr(res, "scenario") <- list(type = "kruskal", groups = groups, mean_ranks = sums / lengths(groups), n_total = n)
  res
}

anova_rm <- function(m) {
  n <- nrow(m)
  k <- ncol(m)
  if (is.null(n) || n < 2 || k < 2) stop("repeated measures ANOVA needs at least two subjects with values in every data set")
  y <- as.vector(m)
  treat <- factor(rep(seq_len(k), each = n))
  subj <- factor(rep(seq_len(n), times = k))
  fit <- stats::aov(y ~ subj + treat)
  tab <- summary(fit)[[1]]
  ss_subj <- tab[1, "Sum Sq"]
  ss_treat <- tab[2, "Sum Sq"]
  ss_err <- tab[3, "Sum Sq"]
  df_s <- tab[1, "Df"]
  df_t <- tab[2, "Df"]
  df_e <- tab[3, "Df"]
  ms_s <- tab[1, "Mean Sq"]
  ms_t <- tab[2, "Mean Sq"]
  ms_e <- tab[3, "Mean Sq"]
  f <- ms_t / ms_e
  p <- stats::pf(f, df_t, df_e, lower.tail = FALSE)
  f_s <- ms_s / ms_e
  p_s <- stats::pf(f_s, df_s, df_e, lower.tail = FALSE)
  # Greenhouse-Geisser epsilon from the covariance matrix of orthonormal contrasts.
  ct <- t(stats::contr.poly(k))
  v <- ct %*% stats::cov(m) %*% t(ct)
  eps <- sum(diag(v))^2 / ((k - 1) * sum(v * v))
  eps <- if (is.finite(eps)) min(max(eps, 1 / (k - 1)), 1) else 1
  res <- list(
    f = f_result(f, df_t, df_e, p),
    epsilon = eps,
    f_gg = f_result(f, eps * df_t, eps * df_e, stats::pf(f, eps * df_t, eps * df_e, lower.tail = FALSE)),
    table = list(
      anova_row("Treatment (between columns)", ss_treat, df_t, ms_t, f, p),
      anova_row("Individual (between rows)", ss_subj, df_s, ms_s, f_s, p_s),
      anova_row("Residual (random)", ss_err, df_e, ms_e),
      anova_row("Total", ss_treat + ss_subj + ss_err, n * k - 1)
    ),
    ms_error = ms_e, df_error = df_e,
    subjects = f_result(f_s, df_s, df_e, p_s),
    r_squared = ss_treat / (ss_treat + ss_err)
  )
  attr(res, "scenario") <- list(type = "rm", m = m, mse = ms_e, df = df_e, residuals = unname(stats::residuals(fit)))
  res
}

anova_friedman <- function(m) {
  n <- nrow(m)
  k <- ncol(m)
  if (is.null(n) || n < 2 || k < 2) stop("the Friedman test needs at least two subjects with values in every data set")
  ft <- stats::friedman.test(m)
  sums <- colSums(t(apply(m, 1, rank)))
  q <- unname(ft$statistic)
  res <- list(q = q, df = unname(ft$parameter), p = ft$p.value,
    mean_ranks = arr(sums / n), rank_sums = arr(sums), subjects = n,
    kendalls_w = q / (n * (k - 1)))
  attr(res, "scenario") <- list(type = "friedman", k = k, n = n, mean_ranks = sums / n)
  res
}

# --- Multiple comparisons ------------------------------------------------------------

# Difference, standard error and degrees of freedom of each pair (difference = a - b).
pair_inputs <- function(pairs, s, sphericity) {
  lapply(pairs, function(p) {
    i <- p[1]
    j <- p[2]
    switch(s$type,
      pooled = {
        gi <- s$groups[[i]]
        gj <- s$groups[[j]]
        list(diff = mean(gi) - mean(gj), se = sqrt(s$mse * (1 / length(gi) + 1 / length(gj))), df = s$df)
      },
      welch = {
        gi <- s$groups[[i]]
        gj <- s$groups[[j]]
        vi <- stats::var(gi) / length(gi)
        vj <- stats::var(gj) / length(gj)
        df <- (vi + vj)^2 / (vi^2 / (length(gi) - 1) + vj^2 / (length(gj) - 1))
        list(diff = mean(gi) - mean(gj), se = sqrt(vi + vj), df = df)
      },
      rm = {
        if (sphericity) {
          n <- nrow(s$m)
          list(diff = mean(s$m[, i]) - mean(s$m[, j]), se = sqrt(s$mse * 2 / n), df = s$df)
        } else {
          # Geisser-Greenhouse: each pair from its own differences.
          d <- s$m[, i] - s$m[, j]
          list(diff = mean(d), se = stats::sd(d) / sqrt(length(d)), df = length(d) - 1)
        }
      },
      kruskal = {
        ni <- length(s$groups[[i]])
        nj <- length(s$groups[[j]])
        list(diff = s$mean_ranks[i] - s$mean_ranks[j], se = sqrt(s$n_total * (s$n_total + 1) / 12 * (1 / ni + 1 / nj)), df = Inf)
      },
      friedman = list(diff = s$mean_ranks[i] - s$mean_ranks[j], se = sqrt(s$k * (s$k + 1) / (6 * s$n)), df = Inf)
    )
  })
}

group_sizes <- function(s) {
  switch(s$type, pooled = lengths(s$groups), welch = lengths(s$groups), kruskal = lengths(s$groups),
    rm = rep(nrow(s$m), ncol(s$m)), friedman = rep(s$n, s$k))
}

multiple_comparisons <- function(pairs, method, s, alpha, sphericity) {
  inputs <- pair_inputs(pairs, s, sphericity)
  m <- length(inputs)
  k <- switch(s$type, rm = ncol(s$m), friedman = s$k, length(s$groups))
  conf <- 1 - alpha
  tval <- vapply(inputs, function(x) x$diff / x$se, 0)
  dfs <- vapply(inputs, function(x) x$df, 0)
  raw <- ifelse(is.finite(dfs), 2 * stats::pt(-abs(tval), dfs), 2 * stats::pnorm(-abs(tval)))
  adj <- rep(NA_real_, m)
  half <- rep(NA_real_, m)
  if (method %in% c("tukey", "games_howell")) {
    adj <- stats::ptukey(sqrt(2) * abs(tval), k, dfs, lower.tail = FALSE)
    half <- stats::qtukey(conf, k, dfs) / sqrt(2) * vapply(inputs, `[[`, 0, "se")
  } else if (method == "dunnett") {
    # Comparisons with a shared control are correlated by lambda_i * lambda_j
    # (lambda_i = sqrt(n_i / (n_i + n_control))).
    ns <- group_sizes(s)
    control <- pairs[[1]][1]
    others <- vapply(pairs, `[`, 0L, 2)
    lambda <- if (s$type == "pooled") sqrt(ns[others] / (ns[others] + ns[control])) else rep(sqrt(0.5), m)
    crit <- list()
    for (i in seq_len(m)) {
      adj[i] <- 1 - pdunnett(abs(tval[i]), lambda, dfs[i])
      key <- as.character(dfs[i])
      if (is.null(crit[[key]])) crit[[key]] <- qdunnett(conf, lambda, dfs[i])
      half[i] <- crit[[key]] * inputs[[i]]$se
    }
  } else if (method == "sidak") {
    adj <- 1 - (1 - raw)^m
    half <- vapply(seq_len(m), function(i) t_crit(conf^(1 / m), dfs[i]) * inputs[[i]]$se, 0)
  } else if (method == "dunnett_t3") {
    adj <- vapply(seq_len(m), function(i) 1 - psmm(abs(tval[i]), m, dfs[i]), 0)
    half <- vapply(seq_len(m), function(i) qsmm(conf, m, dfs[i]) * inputs[[i]]$se, 0)
  } else if (method == "dunn") {
    adj <- pmin(raw * m, 1)
  } else if (method == "uncorrected") {
    adj <- raw
    half <- vapply(seq_len(m), function(i) t_crit(conf, dfs[i]) * inputs[[i]]$se, 0)
  } else if (method == "fdr") {
    f <- two_stage_fdr(raw, alpha)
    adj <- f$q
  } else {
    stop("unknown multiple comparisons method '", method, "'")
  }
  adj <- pmin(pmax(adj, 0), 1)
  significant <- if (method == "fdr") two_stage_fdr(raw, alpha)$discovered else adj < alpha
  lapply(seq_len(m), function(i) {
    d <- inputs[[i]]$diff
    list(a = pairs[[i]][1] - 1, b = pairs[[i]][2] - 1, diff = d,
      ci = if (is.finite(half[i])) arr(c(d - half[i], d + half[i])) else NULL,
      p_adjusted = adj[i], p_raw = raw[i], significant = significant[i])
  })
}

# Dunnett's distribution: P(max |T_i| <= x) for t statistics on df degrees of freedom whose
# correlations are lambda_i * lambda_j (Dunnett 1955, J Am Stat Assoc 50:1096). The
# one-factor structure reduces the multivariate t probability to a double integral over
# the common normal factor z and the chi distribution of the pooled SD.
pdunnett <- function(x, lambda, df) {
  inner <- function(s) vapply(s, function(si) {
    stats::integrate(function(z) {
      out <- stats::dnorm(z)
      for (l in lambda) {
        r <- sqrt(1 - l^2)
        out <- out * (stats::pnorm((x * si + l * z) / r) - stats::pnorm((-x * si + l * z) / r))
      }
      out
    }, -Inf, Inf, rel.tol = 1e-10)$value
  }, 0)
  if (!is.finite(df)) return(inner(1))
  stats::integrate(function(s) inner(s) * 2 * df * s * stats::dchisq(df * s^2, df), 0, Inf, rel.tol = 1e-9)$value
}

qdunnett <- function(conf, lambda, df) stats::uniroot(function(x) pdunnett(x, lambda, df) - conf, c(1, 10), tol = 1e-8)$root

# Studentized maximum modulus: P(max of m independent |t| <= x) with a shared denominator
# on df degrees of freedom (Dunnett's T3).
psmm <- function(x, m, df) {
  if (!is.finite(df)) return((2 * stats::pnorm(x) - 1)^m)
  f <- function(s) (2 * stats::pnorm(x * s) - 1)^m * 2 * df * s * stats::dchisq(df * s^2, df)
  stats::integrate(f, 0, Inf, rel.tol = 1e-10)$value
}

qsmm <- function(conf, m, df) stats::uniroot(function(x) psmm(x, m, df) - conf, c(0.01, 100), tol = 1e-10)$root

# Benjamini-Hochberg step-up at level q: which hypotheses are rejected.
bh_reject <- function(p, q) {
  m <- length(p)
  o <- order(p)
  ok <- which(p[o] <= seq_len(m) * q / m)
  out <- rep(FALSE, m)
  if (length(ok) > 0) out[o[seq_len(max(ok))]] <- TRUE
  out
}

# Two-stage linear step-up procedure of Benjamini, Krieger & Yekutieli (2006).
two_stage <- function(p, q) {
  m <- length(p)
  q1 <- q / (1 + q)
  first <- bh_reject(p, q1)
  r1 <- sum(first)
  if (r1 == 0 || r1 == m) return(first)
  bh_reject(p, q1 * m / (m - r1))
}

# Discoveries at q plus each comparison's q value: the smallest Q at which it is a discovery.
two_stage_fdr <- function(p, q) {
  at_one <- two_stage(p, 1)
  qv <- vapply(seq_along(p), function(i) {
    if (!at_one[i]) return(1)
    lo <- 0
    hi <- 1
    for (step in 1:50) {
      mid <- (lo + hi) / 2
      if (two_stage(p, mid)[i]) hi <- mid else lo <- mid
    }
    hi
  }, 0)
  list(q = qv, discovered = two_stage(p, q))
}
