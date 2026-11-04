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
    outliers = op_outliers(data, options),
    correlation = op_correlation(data, options),
    linear_regression = op_linear_regression(data, options),
    curve_fit = op_curve_fit(data, options),
    two_way = op_two_way(data, options),
    auc = op_auc(data, options),
    survival = op_survival(data, options),
    contingency = op_contingency(data, options),
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
