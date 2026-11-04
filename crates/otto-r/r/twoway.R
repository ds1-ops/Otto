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
