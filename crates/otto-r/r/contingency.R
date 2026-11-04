# Contingency table analysis (base R's stats package): the chi-square test with and
# without Yates' correction, Fisher's exact test, McNemar's test for paired 2x2 tables,
# the chi-square test for trend (Cochran-Armitage), and for a 2x2 table the relative risk,
# the odds ratio, the difference between proportions and the diagnostic ratios, as in Prism.

# Wilson score interval for a proportion: behaves at 0 and 1, where Wald does not.
wilson <- function(x, n, z) {
  if (n <= 0) return(c(NA, NA))
  p <- x / n
  d <- 1 + z^2 / n
  centre <- (p + z^2 / (2 * n)) / d
  half <- z * sqrt(p * (1 - p) / n + z^2 / (4 * n^2)) / d
  c(centre - half, centre + half)
}

# A ratio with a log-scale confidence interval; `se_log` is the SE of its logarithm.
ratio_ci <- function(r, se_log, z) {
  if (!is.finite(r) || r <= 0 || !is.finite(se_log)) return(c(NA, NA))
  exp(log(r) + c(-1, 1) * z * se_log)
}

# The chi-square test for trend over equally spaced scores (Cochran-Armitage). `x` are the
# counts in the first row, `n` the column totals; the trend runs along the columns.
trend_test <- function(x, n) {
  N <- sum(n)
  if (N == 0 || length(n) < 3) return(NULL)
  s <- seq_along(n)
  p <- sum(x) / N
  if (p <= 0 || p >= 1) return(NULL)
  t_stat <- sum(s * (x - n * p))
  v <- p * (1 - p) * (sum(n * s^2) - sum(n * s)^2 / N)
  if (!is.finite(v) || v <= 0) return(NULL)
  chi <- t_stat^2 / v
  list(chisq = chi, df = 1, p = stats::pchisq(chi, 1, lower.tail = FALSE))
}

op_contingency <- function(data, o) {
  nr <- as.integer(data$nrow)
  nc <- as.integer(data$ncol)
  counts <- matrix(num(data$counts), nrow = nr, ncol = nc)
  rows <- as.character(unlist(data$rows))
  cols <- as.character(unlist(data$cols))
  if (any(is.na(counts))) stop("every cell of a contingency table needs a count; one of them is empty")
  if (any(counts < 0)) stop("a contingency table counts subjects, so no cell can be negative")
  if (any(abs(counts - round(counts)) > 1e-8)) {
    stop("a contingency table counts subjects, so every cell must be a whole number")
  }
  counts <- round(counts)
  if (sum(counts) == 0) stop("the contingency table is empty: every cell is zero")
  if (nr < 2 || nc < 2) stop(sprintf("a contingency table needs at least two rows and two columns (this one is %d x %d)", nr, nc))
  dimnames(counts) <- list(rows, cols)

  conf <- o$confidence %||% 0.95
  z <- stats::qnorm(1 - (1 - conf) / 2)
  out <- list(
    rows = I(rows), cols = I(cols), nrow = nr, ncol = nc,
    n = sum(counts), confidence = conf,
    row_totals = arr(rowSums(counts)), col_totals = arr(colSums(counts))
  )

  # Pearson chi-square, and the expected counts it assumes (Prism warns when one is small).
  pearson <- suppressWarnings(stats::chisq.test(counts, correct = FALSE))
  out$chi_square <- list(chisq = unname(pearson$statistic), df = unname(pearson$parameter), p = unname(pearson$p.value))
  out$expected <- arr(as.vector(t(pearson$expected)))
  out$min_expected <- min(pearson$expected)

  if (nr == 2 && nc == 2) {
    yates <- suppressWarnings(stats::chisq.test(counts, correct = TRUE))
    out$chi_square_yates <- list(chisq = unname(yates$statistic), df = unname(yates$parameter), p = unname(yates$p.value))
  }

  # Fisher's exact test. It is exact but can need a lot of memory on a big table, so a
  # table it cannot do is reported as not done rather than failing the whole analysis.
  fisher <- tryCatch(stats::fisher.test(counts, conf.level = conf), error = function(e) {
    tryCatch(stats::fisher.test(counts, simulate.p.value = TRUE, B = 1e5), error = function(e2) NULL)
  })
  if (!is.null(fisher)) {
    out$fisher <- list(
      p = unname(fisher$p.value),
      simulated = isTRUE(grepl("simulated", fisher$method)),
      odds_ratio = if (!is.null(fisher$estimate)) unname(fisher$estimate) else NULL,
      odds_ratio_ci = if (!is.null(fisher$conf.int)) arr(as.numeric(fisher$conf.int)) else NULL
    )
  }

  if (isTRUE(o$paired)) {
    if (nr != nc) stop("McNemar's test needs a square table: the same categories before and after")
    mc <- stats::mcnemar.test(counts, correct = TRUE)
    out$mcnemar <- list(chisq = unname(mc$statistic), df = unname(mc$parameter), p = unname(mc$p.value))
  }

  # Trend along whichever side has more than two ordered categories.
  if (nr == 2 && nc > 2) {
    out$trend <- trend_test(counts[1, ], colSums(counts))
    out$trend_along <- "columns"
  } else if (nc == 2 && nr > 2) {
    out$trend <- trend_test(counts[, 1], rowSums(counts))
    out$trend_along <- "rows"
  }

  if (nr == 2 && nc == 2) {
    a <- counts[1, 1]; b <- counts[1, 2]; c_ <- counts[2, 1]; d <- counts[2, 2]
    r1 <- a + b; r2 <- c_ + d
    p1 <- a / r1
    p2 <- c_ / r2
    rr <- p1 / p2
    se_rr <- sqrt(1 / a - 1 / r1 + 1 / c_ - 1 / r2)
    or <- (a * d) / (b * c_)
    se_or <- sqrt(1 / a + 1 / b + 1 / c_ + 1 / d)
    diff <- p1 - p2
    se_diff <- sqrt(p1 * (1 - p1) / r1 + p2 * (1 - p2) / r2)
    out$effect <- list(
      p1 = p1, p1_ci = arr(wilson(a, r1, z)),
      p2 = p2, p2_ci = arr(wilson(c_, r2, z)),
      relative_risk = rr, relative_risk_ci = arr(ratio_ci(rr, se_rr, z)),
      odds_ratio = or, odds_ratio_ci = arr(ratio_ci(or, se_or, z)),
      difference = diff, difference_ci = arr(diff + c(-1, 1) * z * se_diff)
    )
    # The diagnostic ratios, on Prism's convention: the first column is the condition
    # being present and the first row is the test calling it present.
    out$diagnostic <- list(
      sensitivity = a / (a + c_), sensitivity_ci = arr(wilson(a, a + c_, z)),
      specificity = d / (b + d), specificity_ci = arr(wilson(d, b + d, z)),
      ppv = a / r1, ppv_ci = arr(wilson(a, r1, z)),
      npv = d / r2, npv_ci = arr(wilson(d, r2, z)),
      likelihood_positive = (a / (a + c_)) / (1 - d / (b + d)),
      likelihood_negative = (1 - a / (a + c_)) / (d / (b + d))
    )
  }
  out
}
