# R4 "Two-way ANOVA (genotype x treatment)": Two-way ANOVA (full model) (R 4.6.1 · stats · car 3.1.5)
# Reproduces Otto's statistics in R: the functions below are exactly the code Otto runs,
# and the data are the values Otto analysed.
# Packages: jsonlite, car, nortest, minpack.lm, survival (Otto installs them from https://packagemanager.posit.co/cran/2026-10-01).
# Otto's package library; remove this line to use your own packages.
otto_library <- file.path(Sys.getenv("LOCALAPPDATA"), "Otto", "r", "library")
if (dir.exists(otto_library)) .libPaths(c(otto_library, .libPaths()))

# ---- common.R ----
# Shared helpers of Otto's R code: request parsing, JSON output, dispatch and provenance.

`%||%` <- function(a, b) if (is.null(a)) b else a

parse_request <- function(line) {
  jsonlite::fromJSON(line, simplifyVector = TRUE, simplifyDataFrame = FALSE, simplifyMatrix = FALSE)
}

# Non-finite numbers become NA (JSON null), recursively.
clean <- function(x) {
  if (is.list(x)) {
    nm <- names(x)
    x <- lapply(x, clean)
    names(x) <- nm
    return(x)
  }
  if (is.numeric(x)) x[!is.finite(x)] <- NA
  x
}

to_json <- function(x) {
  as.character(jsonlite::toJSON(clean(x), auto_unbox = TRUE, digits = NA, null = "null", na = "null"))
}

# A JSON array of numbers as a numeric vector (an empty array arrives as list()).
num <- function(v) {
  if (is.null(v) || length(v) == 0) return(numeric(0))
  as.numeric(unlist(v))
}

# Keeps a vector a JSON array even when it has one element.
arr <- function(v) I(as.numeric(v))

# Packages used for the current request, reported with their versions.
.otto_used <- character(0)

use_pkg <- function(p) {
  if (!p %in% .otto_used) .otto_used <<- c(.otto_used, p)
  invisible(TRUE)
}

pkg_version <- function(p) tryCatch(as.character(utils::packageVersion(p)), error = function(e) "?")

engine_info <- function() {
  parts <- c(paste("R", getRversion()), "stats", vapply(.otto_used, function(p) paste(p, pkg_version(p)), ""))
  paste(parts, collapse = " · ")
}

dispatch <- function(op, data, options) {
  .otto_used <<- character(0)
  options <- options %||% list()
  switch(op,
    ping = list(r = as.character(getRversion())),
    warm = {
      for (p in c("car", "nortest", "minpack.lm", "survival")) loadNamespace(p)
      list(r = as.character(getRversion()))
    },
    t_test = op_t_test(data, options),
    anova = op_anova(data, options),
    normality = op_normality(data, options),
    correlation = op_correlation(data, options),
    linear_regression = op_linear_regression(data, options),
    curve_fit = op_curve_fit(data, options),
    two_way = op_two_way(data, options),
    auc = op_auc(data, options),
    survival = op_survival(data, options),
    models = op_models(),
    stop("unknown operation '", op, "'")
  )
}

need <- function(v, n, name) {
  if (length(v) < n) stop(sprintf("%s needs at least %d values (has %d)", name, n, length(v)))
}

log10_positive <- function(v, name) {
  if (any(v <= 0)) stop(sprintf("%s contains zero or negative values; a lognormal test needs positive values", name))
  log10(v)
}

# Akaike's information criterion, corrected for small samples, from a sum of squares.
aicc <- function(ss, n, k) n * log(ss / n) + 2 * k + 2 * k * (k + 1) / (n - k - 1)

# Compares a simpler model (k_simple parameters) with a more complex one.
aicc_compare <- function(ss_simple, k_simple, ss_complex, k_complex, n) {
  if (n - k_complex - 1 <= 0 || ss_complex <= 0 || ss_simple <= 0) return(NULL)
  s <- aicc(ss_simple, n, k_simple)
  c <- aicc(ss_complex, n, k_complex)
  list(simpler = s, complex = c, prob_complex = 1 / (1 + exp(0.5 * (c - s))))
}

# Two-sided critical value of Student's t for a confidence level.
t_crit <- function(conf, df) if (is.finite(df)) stats::qt(1 - (1 - conf) / 2, df) else stats::qnorm(1 - (1 - conf) / 2)

# ---- tests.R ----
# t tests and their nonparametric counterparts (Prism's "t tests (and nonparametric tests)").
# Differences are a - b; Otto flips them when the user asks for b - a.

op_t_test <- function(data, o) {
  a <- num(data$a)
  b <- num(data$b)
  conf <- o$confidence %||% 0.95
  res <- switch(o$test,
    unpaired = t_unpaired(a, b, isTRUE(o$welch), isTRUE(o$log), conf),
    mann_whitney = t_mann_whitney(a, b, conf),
    paired = t_paired(a, b, isTRUE(o$log), conf),
    wilcoxon = t_wilcoxon(a, b, conf),
    stop("unknown t test '", o$test, "'")
  )
  tests <- num_tests(o$normality)
  if (length(tests) > 0 && o$test %in% c("unpaired", "paired")) {
    res$normality <- normality_tests(attr(res, "residuals"), tests)
  }
  attr(res, "residuals") <- NULL
  res
}

num_tests <- function(v) if (is.null(v) || length(v) == 0) character(0) else as.character(unlist(v))

two_group <- function(test, statistic_name, statistic, p, diff, ...) {
  base <- list(
    test = test, statistic_name = statistic_name, statistic = statistic, df = NULL, p_two_tailed = p,
    exact = NULL, diff = diff, se = NULL, ci = NULL, ratio = FALSE, r_squared = NULL, f_test = NULL,
    effect = NULL, aicc = NULL, hodges_lehmann = NULL, extra = list()
  )
  modifyList(base, list(...), keep.null = TRUE)
}

extra_row <- function(label, value) list(label = label, value = value)

t_unpaired <- function(a, b, welch, lognormal, conf) {
  need(a, 2, "the first data set")
  need(b, 2, "the second data set")
  if (lognormal) {
    a <- log10_positive(a, "the first data set")
    b <- log10_positive(b, "the second data set")
  }
  tt <- stats::t.test(a, b, var.equal = !welch, conf.level = conf)
  na <- length(a)
  nb <- length(b)
  t <- unname(tt$statistic)
  df <- unname(tt$parameter)
  diff <- mean(a) - mean(b)
  ci <- as.numeric(tt$conf.int)
  sp <- sqrt(((na - 1) * stats::var(a) + (nb - 1) * stats::var(b)) / (na + nb - 2))
  d <- diff / sp
  effect <- list(cohens_d = d, hedges_g = d * (1 - 3 / (4 * (na + nb) - 9)), glass_delta = diff / stats::sd(b))
  f_test <- NULL
  if (!welch) {
    vt <- stats::var.test(a, b)
    va <- stats::var(a)
    vb <- stats::var(b)
    f_test <- if (va >= vb) list(f = va / vb, dfn = na - 1, dfd = nb - 1, p = vt$p.value) else list(f = vb / va, dfn = nb - 1, dfd = na - 1, p = vt$p.value)
  }
  all <- c(a, b)
  ss_one <- sum((all - mean(all))^2)
  ss_two <- sum((a - mean(a))^2) + sum((b - mean(b))^2)
  test <- paste0("Unpaired t test", if (welch) " with Welch's correction" else "", if (lognormal) " on log-transformed data" else "")
  res <- two_group(test, "t", t, tt$p.value, if (lognormal) 10^diff else diff,
    df = df, se = if (lognormal) NULL else unname(tt$stderr), ci = arr(if (lognormal) 10^ci else ci),
    ratio = lognormal, r_squared = t^2 / (t^2 + df), f_test = f_test, effect = effect,
    aicc = aicc_compare(ss_one, 2, ss_two, 3, na + nb)
  )
  attr(res, "residuals") <- c(a - mean(a), b - mean(b))
  res
}

t_paired <- function(a, b, ratio, conf) {
  if (length(a) != length(b)) stop("paired data sets need the same number of matched values")
  need(a, 2, "a paired test")
  x <- a
  y <- b
  if (ratio) {
    x <- log10_positive(a, "the first data set")
    y <- log10_positive(b, "the second data set")
  }
  tt <- stats::t.test(x, y, paired = TRUE, conf.level = conf)
  d <- x - y
  n <- length(d)
  df <- n - 1
  t <- unname(tt$statistic)
  md <- mean(d)
  ci <- as.numeric(tt$conf.int)
  dz <- md / stats::sd(d)
  # How effective was the pairing: Pearson r between the two data sets, one-tailed P.
  pairing <- tryCatch({
    ct <- stats::cor.test(a, b, alternative = "greater")
    list(r = unname(ct$estimate), p = ct$p.value)
  }, error = function(e) list(r = NA, p = NA))
  res <- two_group(if (ratio) "Ratio paired t test" else "Paired t test", "t", t, tt$p.value, if (ratio) 10^md else md,
    df = df, se = if (ratio) NULL else unname(tt$stderr), ci = arr(if (ratio) 10^ci else ci), ratio = ratio,
    r_squared = t^2 / (t^2 + df),
    effect = list(cohens_d = dz, hedges_g = dz * (1 - 3 / (4 * df - 1)), glass_delta = NULL),
    aicc = aicc_compare(sum(d^2), 1, sum((d - md)^2), 2, n),
    extra = list(extra_row("Correlation coefficient (r)", pairing$r), extra_row("P value (one tailed), pairing", pairing$p))
  )
  attr(res, "residuals") <- d - md
  res
}

# Hodges-Lehmann estimate and confidence interval from wilcox.test (NULL when R can't compute them).
hodges_lehmann <- function(...) {
  tryCatch({
    w <- suppressWarnings(stats::wilcox.test(..., conf.int = TRUE))
    if (is.null(w$conf.int)) NULL else list(estimate = unname(w$estimate), ci = arr(w$conf.int))
  }, error = function(e) NULL)
}

t_mann_whitney <- function(a, b, conf) {
  need(a, 1, "the first data set")
  need(b, 1, "the second data set")
  m <- length(a)
  n <- length(b)
  r <- rank(c(a, b))
  exact <- m < 50 && n < 50 && !any(duplicated(c(a, b)))
  w <- suppressWarnings(stats::wilcox.test(a, b, exact = exact))
  ua <- unname(w$statistic)
  two_group("Mann-Whitney test", "U", min(ua, m * n - ua), w$p.value, stats::median(a) - stats::median(b),
    exact = exact,
    hodges_lehmann = hodges_lehmann(a, b, exact = exact, conf.level = conf),
    extra = list(extra_row("Sum of ranks, first data set", sum(r[seq_len(m)])), extra_row("Sum of ranks, second data set", sum(r[m + seq_len(n)])))
  )
}

t_wilcoxon <- function(a, b, conf) {
  if (length(a) != length(b)) stop("paired data sets need the same number of matched values")
  all <- a - b
  d <- all[all != 0]
  need(d, 1, "the Wilcoxon test (non-zero differences)")
  r <- rank(abs(d))
  w_plus <- sum(r[d > 0])
  w_minus <- sum(r[d < 0])
  exact <- length(d) < 50 && !any(duplicated(abs(d))) && !any(all == 0)
  w <- suppressWarnings(stats::wilcox.test(a, b, paired = TRUE, exact = exact))
  two_group("Wilcoxon matched-pairs signed rank test", "W", w_plus - w_minus, w$p.value, stats::median(all),
    exact = exact,
    hodges_lehmann = hodges_lehmann(a, b, paired = TRUE, exact = exact, conf.level = conf),
    extra = list(extra_row("Sum of positive ranks", w_plus), extra_row("Sum of negative ranks", -w_minus))
  )
}

# ---- anova.R ----
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

# ---- twoway.R ----
# Ordinary two-way ANOVA (Prism's grouped analysis): rows = X categories, columns = data
# sets. Type III sums of squares with sum-to-zero contrasts (car::Anova), so unbalanced
# designs are tested the way Prism and SAS do; multiple comparisons use the residual mean
# square of the full model.

op_two_way <- function(data, o) {
  y <- num(data$y)
  a_levels <- as.character(unlist(data$a_levels))
  b_levels <- as.character(unlist(data$b_levels))
  A <- factor(a_levels[as.integer(unlist(data$a)) + 1L], levels = a_levels)
  B <- factor(b_levels[as.integer(unlist(data$b)) + 1L], levels = b_levels)
  if (length(a_levels) < 2 || length(b_levels) < 2) stop("two-way ANOVA needs at least two X categories and at least two data sets")
  interaction <- !isFALSE(o$interaction)
  counts <- table(A, B)
  if (any(counts == 0)) {
    miss <- which(counts == 0, arr.ind = TRUE)
    stop(sprintf("every combination of X and data set needs values; \"%s | %s\" has none", a_levels[miss[1, 1]], b_levels[miss[1, 2]]))
  }
  use_pkg("car")
  old <- options(contrasts = c("contr.sum", "contr.poly"))
  on.exit(options(old))
  fit <- if (interaction) stats::lm(y ~ A * B) else stats::lm(y ~ A + B)
  df_res <- fit$df.residual
  if (df_res < 1) stop("two-way ANOVA needs replicates: there are no degrees of freedom left for the residuals")
  tab <- car::Anova(fit, type = 3)
  total_ss <- sum((y - mean(y))^2)
  res_ss <- tab["Residuals", "Sum Sq"]
  mse <- res_ss / df_res
  row_of <- function(name, key) {
    list(source = name, ss = tab[key, "Sum Sq"], df = tab[key, "Df"], ms = tab[key, "Sum Sq"] / tab[key, "Df"],
      f = tab[key, "F value"], p = tab[key, "Pr(>F)"], percent = 100 * tab[key, "Sum Sq"] / total_ss)
  }
  rows <- list()
  if (interaction) rows[[length(rows) + 1]] <- row_of("interaction", "A:B")
  rows[[length(rows) + 1]] <- row_of("rows", "A")
  rows[[length(rows) + 1]] <- row_of("columns", "B")
  rows[[length(rows) + 1]] <- list(source = "residual", ss = res_ss, df = df_res, ms = mse, f = NULL, p = NULL, percent = 100 * res_ss / total_ss)
  m <- tapply(y, list(A, B), mean)
  n <- tapply(y, list(A, B), length)
  s <- tapply(y, list(A, B), stats::sd)
  cells <- list()
  for (i in seq_along(a_levels)) for (j in seq_along(b_levels)) {
    cells[[length(cells) + 1]] <- list(a = i - 1, b = j - 1, n = n[i, j], mean = m[i, j], sd = s[i, j])
  }
  list(
    table = rows, mse = mse, df_residual = df_res, total_ss = total_ss, n = length(y), cells = cells,
    comparisons = two_way_comparisons(o$compare %||% "none", m, n, mse, df_res, o$method %||% "tukey", o$alpha %||% 0.05)
  )
}

# Adjusted P values and confidence intervals for one family of comparisons.
adjust_family <- function(diff, se, df, method, k, alpha) {
  t <- diff / se
  raw <- 2 * stats::pt(-abs(t), df)
  m <- length(diff)
  conf <- 1 - alpha
  switch(method,
    tukey = list(p = stats::ptukey(sqrt(2) * abs(t), k, df, lower.tail = FALSE), half = stats::qtukey(conf, k, df) / sqrt(2) * se, raw = raw),
    sidak = list(p = 1 - (1 - raw)^m, half = stats::qt(1 - (1 - conf^(1 / m)) / 2, df) * se, raw = raw),
    uncorrected = list(p = raw, half = stats::qt(1 - alpha / 2, df) * se, raw = raw),
    fdr = {
      f <- two_stage_fdr(raw, alpha)
      list(p = f$q, half = rep(NA_real_, m), raw = raw, discovered = f$discovered)
    },
    stop("unknown multiple comparisons method '", method, "'")
  )
}

# Builds the comparisons: within each row (data sets of one X category), within each
# column, between row or column means (averaged over the other factor), or all cells.
# Šídák and FDR treat all comparisons as one family; Tukey corrects within each family.
two_way_comparisons <- function(mode, m, n, mse, df, method, alpha) {
  na <- nrow(m)
  nb <- ncol(m)
  items <- list()
  add <- function(family, a, b, kind, diff, se, k) {
    items[[length(items) + 1]] <<- list(family = family, a = a, b = b, kind = kind, diff = unname(diff), se = unname(se), k = k)
  }
  cell_se <- function(i1, j1, i2, j2) sqrt(mse * (1 / n[i1, j1] + 1 / n[i2, j2]))
  if (mode == "within_rows") {
    for (i in seq_len(na)) for (j1 in seq_len(nb - 1)) for (j2 in (j1 + 1):nb) {
      add(i - 1, c(i - 1, j1 - 1), c(i - 1, j2 - 1), "cells", m[i, j1] - m[i, j2], cell_se(i, j1, i, j2), nb)
    }
  } else if (mode == "within_columns") {
    for (j in seq_len(nb)) for (i1 in seq_len(na - 1)) for (i2 in (i1 + 1):na) {
      add(j - 1, c(i1 - 1, j - 1), c(i2 - 1, j - 1), "cells", m[i1, j] - m[i2, j], cell_se(i1, j, i2, j), na)
    }
  } else if (mode == "row_means") {
    mm <- rowMeans(m)
    v <- mse / nb^2 * rowSums(1 / n)
    for (i1 in seq_len(na - 1)) for (i2 in (i1 + 1):na) add(0, i1 - 1, i2 - 1, "rows", mm[i1] - mm[i2], sqrt(v[i1] + v[i2]), na)
  } else if (mode == "column_means") {
    mm <- colMeans(m)
    v <- mse / na^2 * colSums(1 / n)
    for (j1 in seq_len(nb - 1)) for (j2 in (j1 + 1):nb) add(0, j1 - 1, j2 - 1, "columns", mm[j1] - mm[j2], sqrt(v[j1] + v[j2]), nb)
  } else if (mode == "all_cells") {
    cells <- expand.grid(i = seq_len(na), j = seq_len(nb))
    cells <- cells[order(cells$i, cells$j), ]
    for (c1 in seq_len(nrow(cells) - 1)) for (c2 in (c1 + 1):nrow(cells)) {
      i1 <- cells$i[c1]; j1 <- cells$j[c1]; i2 <- cells$i[c2]; j2 <- cells$j[c2]
      add(0, c(i1 - 1, j1 - 1), c(i2 - 1, j2 - 1), "cells", m[i1, j1] - m[i2, j2], cell_se(i1, j1, i2, j2), na * nb)
    }
  }
  if (length(items) == 0) return(list())
  family <- vapply(items, function(x) x$family, 0)
  if (method %in% c("sidak", "fdr", "uncorrected")) family[] <- 0
  out <- vector("list", length(items))
  for (f in unique(family)) {
    idx <- which(family == f)
    adj <- adjust_family(vapply(items[idx], `[[`, 0, "diff"), vapply(items[idx], `[[`, 0, "se"), df, method, items[[idx[1]]]$k, alpha)
    sig <- if (method == "fdr") adj$discovered else adj$p < alpha
    for (t in seq_along(idx)) {
      it <- items[[idx[t]]]
      out[[idx[t]]] <- list(family = f, kind = it$kind, a = arr(it$a), b = arr(it$b), diff = it$diff,
        ci = if (is.finite(adj$half[t])) arr(c(it$diff - adj$half[t], it$diff + adj$half[t])) else NULL,
        p_adjusted = min(max(adj$p[t], 0), 1), p_raw = adj$raw[t], significant = sig[t])
    }
  }
  out
}

# ---- survival.R ----
# Kaplan-Meier survival analysis (R's survival package): survival proportions with
# Greenwood standard errors and log-log confidence intervals, median survival, the
# log-rank (Mantel-Cox) test, the log-rank test for trend and the Gehan-Breslow-Wilcoxon
# test, and hazard ratios for two curves, as in Prism.

op_survival <- function(data, o) {
  use_pkg("survival")
  time <- num(data$time)
  event <- num(data$event)
  labels <- as.character(unlist(data$labels))
  group <- factor(labels[as.integer(unlist(data$group)) + 1L], levels = labels)
  if (length(time) < 2) stop("survival analysis needs at least two subjects")
  if (!all(event %in% c(0, 1))) stop("events must be 1 (event) or 0 (censored)")
  conf <- o$confidence %||% 0.95
  k <- nlevels(group)
  fit <- if (k > 1) {
    survival::survfit(survival::Surv(time, event) ~ group, conf.int = conf, conf.type = "log-log")
  } else {
    survival::survfit(survival::Surv(time, event) ~ 1, conf.int = conf, conf.type = "log-log")
  }
  s <- summary(fit, censored = TRUE)
  strata <- if (k > 1) match(sub("^group=", "", as.character(s$strata)), labels) else rep(1L, length(s$time))
  med <- stats::quantile(fit, probs = 0.5, conf.int = TRUE)
  med_row <- function(x, i) if (is.matrix(x)) unname(x[i, 1]) else unname(x[1])
  curves <- lapply(seq_len(k), function(i) {
    sel <- strata == i
    list(
      label = labels[i],
      subjects = sum(group == labels[i]),
      events = sum(event[group == labels[i]]),
      median = med_row(med$quantile, i),
      median_ci = arr(c(med_row(med$lower, i), med_row(med$upper, i))),
      time = arr(s$time[sel]), at_risk = arr(s$n.risk[sel]), n_event = arr(s$n.event[sel]), n_censor = arr(s$n.censor[sel]),
      # Survival as a fraction, with the standard error of the survival (Greenwood).
      survival = arr(s$surv[sel]), se = arr(s$std.err[sel]),
      lower = arr(s$lower[sel]), upper = arr(s$upper[sel])
    )
  })
  out <- list(curves = curves, confidence = conf)
  if (k > 1) {
    sd0 <- survival::survdiff(survival::Surv(time, event) ~ group, rho = 0)
    out$logrank <- list(chisq = unname(sd0$chisq), df = k - 1, p = stats::pchisq(sd0$chisq, k - 1, lower.tail = FALSE))
    gbw <- weighted_logrank(time, event, group, function(at_risk, t) at_risk)
    out$gehan_breslow_wilcoxon <- list(chisq = gbw$chisq, df = gbw$df, p = gbw$p)
    if (k > 2) {
      lr <- weighted_logrank(time, event, group, function(at_risk, t) 1)
      sc <- seq_len(k)
      u <- sum(sc * lr$Z)
      v <- drop(t(sc) %*% lr$V %*% sc)
      chi <- u^2 / v
      out$trend <- list(chisq = chi, df = 1, p = stats::pchisq(chi, 1, lower.tail = FALSE))
    }
    if (k == 2) {
      o1 <- sd0$obs[1]; o2 <- sd0$obs[2]; e1 <- sd0$exp[1]; e2 <- sd0$exp[2]
      z <- stats::qnorm(1 - (1 - conf) / 2)
      hr <- (o1 / e1) / (o2 / e2)
      half <- z * sqrt(1 / e1 + 1 / e2)
      v <- sd0$var[1, 1]
      mh <- exp((o1 - e1) / v)
      out$hazard_ratio <- list(
        logrank = hr, logrank_ci = arr(exp(log(hr) + c(-1, 1) * half)),
        mantel_haenszel = mh, mantel_haenszel_ci = arr(exp((o1 - e1) / v + c(-1, 1) * z / sqrt(v))),
        observed = arr(c(o1, o2)), expected = arr(c(e1, e2))
      )
    }
  }
  out
}

# Weighted log-rank test (Klein & Moeschberger, section 7.3): at each event time the
# observed minus expected events per group, weighted by w(at risk, time); w = 1 is the
# log-rank test, w = number at risk the Gehan-Breslow-Wilcoxon test.
weighted_logrank <- function(time, event, group, wfun) {
  k <- nlevels(group)
  z <- numeric(k)
  v <- matrix(0, k, k)
  for (t in sort(unique(time[event == 1]))) {
    at <- vapply(levels(group), function(g) sum(time >= t & group == g), 0)
    d <- vapply(levels(group), function(g) sum(time == t & event == 1 & group == g), 0)
    yt <- sum(at)
    dt <- sum(d)
    w <- wfun(yt, t)
    z <- z + w * (d - at * dt / yt)
    if (yt > 1) {
      p <- at / yt
      v <- v + w^2 * dt * (yt - dt) / (yt - 1) * (diag(p, k) - outer(p, p))
    }
  }
  idx <- seq_len(k - 1)
  chisq <- drop(t(z[idx]) %*% solve(v[idx, idx, drop = FALSE]) %*% z[idx])
  list(chisq = chisq, df = k - 1, p = stats::pchisq(chisq, k - 1, lower.tail = FALSE), Z = z, V = v)
}

# ---- normality.R ----
# Normality tests (Prism's "Normality and Lognormality Tests" and the residuals tabs):
# D'Agostino-Pearson omnibus K2, Anderson-Darling, Shapiro-Wilk and Kolmogorov-Smirnov
# (Lilliefors, with the Dallal-Wilkinson approximation for P).

op_normality <- function(data, o) {
  sets <- lapply(data$sets, num)
  tests <- num_tests(o$tests)
  lapply(sets, function(v) normality_tests(v, tests))
}

normality_tests <- function(v, tests) {
  lapply(tests, function(t) c(list(test = t), normality_test(t, v)))
}

normality_test <- function(test, v) {
  n <- length(v)
  tryCatch(switch(test,
    dagostino = {
      if (n < 8) stop("needs at least 8 values")
      dagostino_pearson(v)
    },
    anderson = {
      if (n < 8) stop("needs at least 8 values")
      use_pkg("nortest")
      x <- nortest::ad.test(v)
      list(statistic = unname(x$statistic), p = x$p.value)
    },
    shapiro = {
      if (n < 3) stop("needs at least 3 values")
      if (n > 5000) stop("needs at most 5000 values")
      x <- stats::shapiro.test(v)
      list(statistic = unname(x$statistic), p = x$p.value)
    },
    ks = {
      if (n < 5) stop("needs at least 5 values")
      use_pkg("nortest")
      x <- nortest::lillie.test(v)
      list(statistic = unname(x$statistic), p = x$p.value)
    },
    stop("unknown normality test '", test, "'")
  ), error = function(e) list(statistic = NA, p = NA, note = conditionMessage(e)))
}

# D'Agostino, Belanger & D'Agostino (1990), "A suggestion for using powerful and
# informative tests of normality", The American Statistician 44:316-321.
dagostino_pearson <- function(x) {
  n <- length(x)
  d <- x - mean(x)
  m2 <- mean(d^2)
  if (m2 == 0) stop("all values are identical")
  b1 <- mean(d^3) / m2^1.5
  b2 <- mean(d^4) / m2^2
  # Skewness.
  y <- b1 * sqrt((n + 1) * (n + 3) / (6 * (n - 2)))
  beta2 <- 3 * (n^2 + 27 * n - 70) * (n + 1) * (n + 3) / ((n - 2) * (n + 5) * (n + 7) * (n + 9))
  w2 <- -1 + sqrt(2 * (beta2 - 1))
  delta <- 1 / sqrt(log(sqrt(w2)))
  alpha <- sqrt(2 / (w2 - 1))
  z1 <- delta * log(y / alpha + sqrt((y / alpha)^2 + 1))
  # Kurtosis (Anscombe & Glynn 1983).
  e_b2 <- 3 * (n - 1) / (n + 1)
  v_b2 <- 24 * n * (n - 2) * (n - 3) / ((n + 1)^2 * (n + 3) * (n + 5))
  xk <- (b2 - e_b2) / sqrt(v_b2)
  sb1 <- 6 * (n^2 - 5 * n + 2) / ((n + 7) * (n + 9)) * sqrt(6 * (n + 3) * (n + 5) / (n * (n - 2) * (n - 3)))
  a <- 6 + 8 / sb1 * (2 / sb1 + sqrt(1 + 4 / sb1^2))
  term <- (1 - 2 / a) / (1 + xk * sqrt(2 / (a - 4)))
  z2 <- ((1 - 2 / (9 * a)) - sign(term) * abs(term)^(1 / 3)) / sqrt(2 / (9 * a))
  k2 <- z1^2 + z2^2
  list(statistic = k2, p = stats::pchisq(k2, 2, lower.tail = FALSE), skewness = b1, kurtosis = b2 - 3)
}

# ---- xy.R ----
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

# ---- the analysis ----
request1 <- parse_request('{"op":"two_way","data":{"a":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],"a_levels":["WT","Tnf-KO"],"b":[0,0,0,0,0,0,0,0,1,1,1,1,1,1,1,1,0,0,0,0,0,0,0,0,1,1,1,1,1,1,1,1],"b_levels":["Vehicle","Cmpd A"],"y":[7.82,7.96,7.67,7.32,7.79,7.9,6.17,5.8,1.44,4.04,3.54,2.8,4.77,2.99,3.28,3.36,3.95,1.22,2.93,2.86,4.24,3.29,5.97,4.07,4.22,3.83,4.82,5.09,4.63,4.43,3.87,3.05]},"options":{"alpha":0.05,"compare":"within_rows","interaction":true,"method":"sidak"}}')
result1 <- dispatch(request1$op, request1$data, request1$options)
str(result1, digits.d = 10)
cat("\nComputed with", engine_info(), "\n")
