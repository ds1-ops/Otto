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
  given <- list(...)
  res <- modifyList(base, given, keep.null = TRUE)
  # `extra` is an *unnamed* list of rows; modifyList() would recurse into it and keep the
  # empty default, so it is carried over as a whole.
  if (!is.null(given$extra)) res$extra <- unname(given$extra)
  res
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
