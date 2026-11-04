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
