# Outlier identification: Grubbs' test and the generalized extreme studentized deviate
# (ESD) procedure of Rosner (1983), which finds several outliers without masking.

op_outliers <- function(data, o) {
  sets <- lapply(data$sets, num)
  alpha <- o$alpha %||% 0.05
  method <- o$method %||% "esd"
  max_out <- o$max_outliers %||% 5
  lapply(sets, function(v) outliers_of(v, method, alpha, max_out))
}

outliers_of <- function(v, method, alpha, max_out) {
  v <- v[is.finite(v)]
  n <- length(v)
  if (method == "tukey") {
    if (n < 4) return(list(test = "Tukey (1.5 x IQR)", n = n, note = "needs at least 4 values", found = list()))
    q <- stats::quantile(v, c(0.25, 0.75), type = 6, names = FALSE)
    iqr <- q[2] - q[1]
    idx <- which(v < q[1] - 1.5 * iqr | v > q[2] + 1.5 * iqr)
    return(list(
      test = "Tukey (1.5 x IQR)", n = n, lower = q[1] - 1.5 * iqr, upper = q[2] + 1.5 * iqr,
      found = lapply(idx, function(i) list(value = v[i], g = NA, critical = NA, p = NA))
    ))
  }
  limit <- if (method == "grubbs") 1 else max(1, min(max_out, n - 2))
  if (n < 3) return(list(test = grubbs_name(method), n = n, note = "needs at least 3 values", found = list()))
  # Rosner's procedure: remove the most extreme value, test it, repeat.
  rest <- v
  found <- list()
  for (k in seq_len(limit)) {
    m <- length(rest)
    if (m < 3) break
    mu <- mean(rest)
    s <- stats::sd(rest)
    if (!is.finite(s) || s == 0) break
    i <- which.max(abs(rest - mu))
    g <- abs(rest[i] - mu) / s
    crit <- grubbs_critical(m, alpha)
    p <- grubbs_p(g, m)
    found[[k]] <- list(value = rest[i], g = g, critical = crit, p = p, significant = g > crit)
    rest <- rest[-i]
  }
  # Rosner: the number of outliers is the largest k whose statistic still exceeded its critical value.
  last <- 0
  for (k in seq_along(found)) if (isTRUE(found[[k]]$significant)) last <- k
  list(test = grubbs_name(method), n = n, outliers = last, found = found)
}

grubbs_name <- function(method) if (method == "grubbs") "Grubbs' test" else "Generalized ESD (iterative Grubbs)"

# Two-sided critical value of Grubbs' statistic at level alpha for n values.
grubbs_critical <- function(n, alpha) {
  t <- stats::qt(alpha / (2 * n), n - 2, lower.tail = FALSE)
  (n - 1) / sqrt(n) * sqrt(t^2 / (n - 2 + t^2))
}

# Two-sided P value of Grubbs' statistic (exact under the same relation as the critical value).
grubbs_p <- function(g, n) {
  inner <- n * (n - 2) * g^2 / ((n - 1)^2 - n * g^2)
  if (!is.finite(inner) || inner < 0) return(0)
  min(1, 2 * n * stats::pt(sqrt(inner), n - 2, lower.tail = FALSE))
}
