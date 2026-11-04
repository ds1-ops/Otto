# Checks Otto's R code against established R implementations:
#   Tukey (stats::TukeyHSD), Dunnett (multcomp::glht), Geisser-Greenhouse (anova.mlm),
#   4PL dose-response (drc::drm LL.4 on drc's ryegrass data), nonlinear least squares
#   (stats::nls), and that every curve model recovers its own parameters.
#   Rscript --vanilla cross_check.R <Otto's R source dir> <Otto's package library>

args <- commandArgs(trailingOnly = TRUE)
.libPaths(args[2])
for (f in c("common.R", "tests.R", "anova.R", "twoway.R", "normality.R", "xy.R", "survival.R")) source(file.path(args[1], f))

failures <- 0
check <- function(name, otto, reference, tol) {
  otto <- unname(as.numeric(otto))
  reference <- unname(as.numeric(reference))
  bad <- length(otto) != length(reference) || any(is.na(otto) != is.na(reference)) ||
    any(abs(otto - reference) > tol * pmax(1, abs(reference)), na.rm = TRUE)
  if (bad) {
    failures <<- failures + 1
    cat("FAIL ", name, ": Otto ", paste(signif(otto, 8), collapse = " "), " vs. ", paste(signif(reference, 8), collapse = " "), "\n", sep = "")
  } else {
    cat("ok   ", name, "\n", sep = "")
  }
}
field <- function(list, name) vapply(list, function(x) as.numeric(x[[name]] %||% NA), 0)

# --- Tukey and Fisher's LSD vs. TukeyHSD / pairwise.t.test ---------------------------
groups <- list(c(4.2, 5.1, 3.9, 4.8), c(5.9, 6.3, 5.2, 6.8, 6.1), c(7.4, 8.1, 6.9))
y <- unlist(groups)
g <- factor(rep(1:3, lengths(groups)))
pairs <- list(c(0, 1), c(0, 2), c(1, 2))
res <- op_anova(list(groups = groups, pairs = pairs), list(design = "ordinary", method = "tukey", alpha = 0.05))
th <- stats::TukeyHSD(stats::aov(y ~ g))$g[c("2-1", "3-1", "3-2"), ]
check("Tukey-Kramer adjusted P", field(res$comparisons, "p_adjusted"), th[, "p adj"], 1e-6)
check("Tukey-Kramer difference", field(res$comparisons, "diff"), -th[, "diff"], 1e-10)
check("Tukey-Kramer CI", sapply(res$comparisons, function(c) c$ci[1]), -th[, "upr"], 1e-6)
res <- op_anova(list(groups = groups, pairs = pairs), list(design = "ordinary", method = "uncorrected", alpha = 0.05))
pw <- stats::pairwise.t.test(y, g, p.adjust.method = "none", pool.sd = TRUE)$p.value
check("Fisher's LSD P", field(res$comparisons, "p_raw"), c(pw["2", "1"], pw["3", "1"], pw["3", "2"]), 1e-8)
res <- op_anova(list(groups = groups, pairs = pairs), list(design = "ordinary", method = "sidak", alpha = 0.05))
check("Sidak P", field(res$comparisons, "p_adjusted"), 1 - (1 - c(pw["2", "1"], pw["3", "1"], pw["3", "2"]))^3, 1e-8)

# --- Ordinary ANOVA, Welch, Kruskal-Wallis vs. their R functions -------------------------
res <- op_anova(list(groups = groups), list(design = "ordinary"))
a <- summary(stats::aov(y ~ g))[[1]]
check("ANOVA F", res$f$f, a[1, "F value"], 1e-10)
check("ANOVA P", res$f$p, a[1, "Pr(>F)"], 1e-10)
res <- op_anova(list(groups = groups), list(design = "welch"))
w <- stats::oneway.test(y ~ g)
check("Welch ANOVA", c(res$welch$f, res$welch$df2, res$welch$p), c(w$statistic, w$parameter[2], w$p.value), 1e-10)

# --- Dunnett vs. multcomp ----------------------------------------------------------------
if (requireNamespace("multcomp", quietly = TRUE)) {
  res <- op_anova(list(groups = groups, pairs = list(c(0, 1), c(0, 2))), list(design = "ordinary", method = "dunnett", alpha = 0.05))
  set.seed(1)
  gl <- summary(multcomp::glht(stats::aov(y ~ g), linfct = multcomp::mcp(g = "Dunnett")))
  check("Dunnett adjusted P (multcomp)", field(res$comparisons, "p_adjusted"), gl$test$pvalues, 2e-4)
  set.seed(1)
  ci <- stats::confint(multcomp::glht(stats::aov(y ~ g), linfct = multcomp::mcp(g = "Dunnett")))$confint
  check("Dunnett CI (multcomp)", sapply(res$comparisons, function(c) c$ci[2]), -ci[, "lwr"], 2e-3)
} else {
  cat("skip Dunnett: multcomp not installed\n")
}

# Dunnett's published two-sided 5% critical values (Dunnett 1964, Biometrics 20:482):
# 4 comparisons with 20 df -> 2.65; 2 comparisons with 10 df -> 2.57.
check("Dunnett critical value (table, k=4, df=20)", round(qdunnett(0.95, rep(sqrt(0.5), 4), 20), 2), 2.65, 1e-9)
check("Dunnett critical value (table, k=2, df=10)", round(qdunnett(0.95, rep(sqrt(0.5), 2), 10), 2), 2.57, 1e-9)

# --- Repeated measures: Geisser-Greenhouse vs. anova.mlm ---------------------------------
m <- rbind(c(1, 3, 4, 3.5), c(2, 3, 6, 5), c(2, 5, 5, 4.2), c(3, 4, 7, 6.1), c(2.5, 3.3, 5.8, 4.4))
res <- op_anova(list(rows = lapply(seq_len(nrow(m)), function(i) m[i, ])), list(design = "rm"))
mlm <- stats::anova(stats::lm(m ~ 1), X = ~1, test = "Spherical")
check("RM ANOVA F", res$f$f, mlm[1, "F"], 1e-8)
check("Geisser-Greenhouse P", res$f_gg$p, mlm[1, "G-G Pr"], 1e-8)
fr <- stats::friedman.test(m)
res <- op_anova(list(rows = lapply(seq_len(nrow(m)), function(i) m[i, ])), list(design = "friedman"))
check("Friedman", c(res$q, res$p), c(fr$statistic, fr$p.value), 1e-10)

# --- 4PL dose-response vs. drc (ryegrass) ------------------------------------------------
if (requireNamespace("drc", quietly = TRUE)) {
  data("ryegrass", package = "drc", envir = environment())
  fit <- drc::drm(rootl ~ conc, data = ryegrass, fct = drc::LL.4())
  co <- stats::coef(fit)
  # drc's LL.4: c + (d - c) / (1 + exp(b (log x - log e))) = Otto's [inhibitor] vs. response.
  res <- op_curve_fit(list(x = ryegrass$conc, y = ryegrass$rootl), list(model = "inhibitor_conc", confidence = 0.95))
  p <- setNames(field(res$params, "value"), vapply(res$params, `[[`, "", "name"))
  s <- setNames(field(res$params, "se"), vapply(res$params, `[[`, "", "name"))
  check("4PL vs. drc: IC50 (ED50)", p["IC50"], co["e:(Intercept)"], 1e-4)
  check("4PL vs. drc: Hill slope (b)", p["HillSlope"], co["b:(Intercept)"], 1e-4)
  check("4PL vs. drc: Bottom, Top (c, d)", p[c("Bottom", "Top")], co[c("c:(Intercept)", "d:(Intercept)")], 1e-4)
  # Standard errors: asymptotic (Gauss-Newton), as nls and Prism report them; drc's
  # summary uses its own Hessian-based estimate and differs by about 1%.
  ref <- stats::nls(rootl ~ c + (d - c) / (1 + (conc / e)^b), data = ryegrass, start = list(b = 2, c = 0.5, d = 8, e = 3))
  check("4PL on ryegrass vs. nls: standard errors", s[c("HillSlope", "Bottom", "Top", "IC50")], summary(ref)$coefficients[, "Std. Error"], 1e-4)
  ed <- drc::ED(fit, 50, interval = "delta", display = FALSE)
  cat("     drc ED50 = ", signif(ed[1], 6), " (published for ryegrass: 3.058)\n", sep = "")
} else {
  cat("skip drc: not installed\n")
}

# --- Nonlinear least squares vs. stats::nls ----------------------------------------------
x <- seq(-9, -3, by = 0.5)
yn <- c(11.057, 12.545, 11.309, 7.339, 17.464, 27.4, 48.389, 75.683, 86.346, 89.634, 89.768, 91.56, 87.771)
res <- op_curve_fit(list(x = x, y = yn), list(model = "agonist_4pl"))
ref <- stats::nls(yn ~ b + (t - b) / (1 + 10^((l - x) * h)), start = list(b = 10, t = 90, l = -6, h = 1))
check("4PL vs. nls: estimates", field(res$params, "value"), stats::coef(ref), 1e-5)
check("4PL vs. nls: standard errors", field(res$params, "se"), summary(ref)$coefficients[, "Std. Error"], 1e-4)

# --- Two-way ANOVA vs. aov (balanced) and car::Anova (unbalanced) -----------------------
A <- rep(0:2, each = 8)
B <- rep(rep(0:1, each = 4), 3)
y2 <- c(10.2, 11.1, 9.8, 10.6, 13.0, 12.4, 13.9, 12.8, 12.1, 11.7, 12.9, 12.4, 16.3, 15.2, 16.8, 15.9, 14.2, 13.8, 14.9, 14.1, 19.8, 20.4, 18.9, 19.6)
lv <- list(a_levels = c("x1", "x2", "x3"), b_levels = c("g1", "g2"))
res <- op_two_way(c(list(y = y2, a = A, b = B), lv), list(compare = "within_rows", method = "sidak", alpha = 0.05))
fa <- factor(A); fb <- factor(B)
av <- summary(stats::aov(y2 ~ fa * fb))[[1]]
check("two-way (balanced) F: interaction, rows, columns", field(res$table[1:3], "f"), av[c(3, 1, 2), "F value"], 1e-8)
check("two-way (balanced) P", field(res$table[1:3], "p"), av[c(3, 1, 2), "Pr(>F)"], 1e-8)
# Simple effects within each row: pooled-MSE t tests, Sidak over the three rows.
mse <- av["Residuals", "Mean Sq"]; dfr <- av["Residuals", "Df"]
m <- tapply(y2, list(fa, fb), mean)
t_se <- (m[, 1] - m[, 2]) / sqrt(mse * (1 / 4 + 1 / 4))
check("two-way simple effects (Sidak)", field(res$comparisons, "p_adjusted"), 1 - (1 - 2 * stats::pt(-abs(t_se), dfr))^3, 1e-8)
keep <- -c(1, 9, 20)
res <- op_two_way(c(list(y = y2[keep], a = A[keep], b = B[keep]), lv), list(compare = "none"))
op <- options(contrasts = c("contr.sum", "contr.poly"))
ca <- car::Anova(stats::lm(y2[keep] ~ fa[keep] * fb[keep]), type = 3)
options(op)
check("two-way (unbalanced) Type III F", field(res$table[1:3], "f"), ca[c(4, 2, 3), "F value"], 1e-8)

# --- Survival: log-rank vs. survdiff, weighted log-rank machinery, Gehan-Breslow vs. coin --
leuk <- tryCatch(survival::aml, error = function(e) survival::leukemia)
grp <- factor(leuk$x)
sv <- op_survival(list(time = leuk$time, event = leuk$status, group = as.integer(grp) - 1, labels = levels(grp)), list(confidence = 0.95))
sd0 <- survival::survdiff(survival::Surv(time, status) ~ x, data = leuk)
check("log-rank vs. survdiff (rho = 0)", sv$logrank$chisq, sd0$chisq, 1e-10)
check("log-rank, published for the AML data (chi2 = 3.4)", round(sv$logrank$chisq, 1), 3.4, 1e-9)
check("median survival, published (31 and 23 weeks)", c(sv$curves[[1]]$median, sv$curves[[2]]$median), c(31, 23), 1e-9)
fit <- survival::survfit(survival::Surv(time, status) ~ 1, data = leuk)
peto <- weighted_logrank(leuk$time, leuk$status, grp, function(n, t) summary(fit, times = t - 1e-9, extend = TRUE)$surv)
check("weighted log-rank with S(t-) vs. survdiff (rho = 1, Peto-Peto)", peto$chisq, survival::survdiff(survival::Surv(time, status) ~ x, data = leuk, rho = 1)$chisq, 1e-6)
if (requireNamespace("coin", quietly = TRUE)) {
  # coin computes the same scores (weights = number at risk) but standardizes with the
  # permutation variance; Otto, like survdiff, uses the classical (hypergeometric) one.
  lt <- coin::logrank_test(survival::Surv(time, status) ~ grp, data = data.frame(time = leuk$time, status = leuk$status, grp = grp), type = "Gehan-Breslow")
  gb <- weighted_logrank(leuk$time, leuk$status, grp, function(n, t) n)
  check("Gehan-Breslow-Wilcoxon scores vs. coin::logrank_test", abs(gb$Z[1]), abs(coin::statistic(lt, type = "centered")), 1e-8)
  check("Gehan-Breslow-Wilcoxon with coin's variance = coin's statistic", gb$Z[1]^2 / coin::variance(lt), coin::statistic(lt)^2, 1e-8)
} else {
  cat("skip Gehan-Breslow: coin not installed
")
}
s3 <- op_survival(list(time = c(leuk$time, leuk$time + 5), event = c(leuk$status, leuk$status), group = c(as.integer(grp) - 1, rep(2, nrow(leuk))), labels = c(levels(grp), "Later")), list())
sd3 <- survival::survdiff(survival::Surv(c(leuk$time, leuk$time + 5), c(leuk$status, leuk$status)) ~ factor(c(as.integer(grp), rep(3, nrow(leuk)))))
check("log-rank, three curves vs. survdiff", s3$logrank$chisq, sd3$chisq, 1e-8)

# --- Area under the curve ----------------------------------------------------------------
au <- op_auc(list(x = c(0, 1, 2, 3, 4), y = c(1, 3, 5, 3, 1)), list(baseline = 1))
check("AUC: triangle above baseline 1 (area 8)", au$total_area, 8, 1e-12)
au <- op_auc(list(x = c(0, 1, 2, 3, 4), y = c(-1, 1, 1, -1, -1)), list(baseline = 0, peaks_below = TRUE))
check("AUC: net area 0 and three regions (-0.25, +1.5, -1.25)", c(au$total_area, length(au$peaks), vapply(au$peaks, `[[`, 0, "area")), c(0, 3, -0.25, 1.5, -1.25), 1e-12)

# --- Every model recovers its parameters from its own noise-free curve -------------------
truth <- list(
  line = list(x = 0:10, p = c(YIntercept = 2, Slope = 0.5)),
  poly2 = list(x = 0:10, p = c(B0 = 1, B1 = 2, B2 = -0.3)),
  poly3 = list(x = 0:10, p = c(B0 = 1, B1 = 0.5, B2 = -0.2, B3 = 0.02)),
  agonist_1pl = list(x = seq(-9, -3, 0.5), p = c(LogEC50 = -6)),
  agonist_2pl = list(x = seq(-9, -3, 0.5), p = c(LogEC50 = -6, HillSlope = 1.3)),
  agonist_3pl = list(x = seq(-9, -3, 0.5), p = c(Bottom = 5, Top = 95, LogEC50 = -6)),
  agonist_4pl = list(x = seq(-9, -3, 0.5), p = c(Bottom = 5, Top = 95, LogEC50 = -6, HillSlope = 1.3)),
  agonist_5pl = list(x = seq(-9, -3, 0.25), p = c(Bottom = 5, Top = 95, LogEC50 = -6, HillSlope = 1.3, S = 0.6)),
  agonist_conc = list(x = c(0.1, 0.3, 1, 3, 10, 30, 100), p = c(Bottom = 2, Top = 50, EC50 = 3, HillSlope = 1.5)),
  inhibitor_1pl = list(x = seq(-9, -3, 0.5), p = c(LogIC50 = -6)),
  inhibitor_2pl = list(x = seq(-9, -3, 0.5), p = c(LogIC50 = -6, HillSlope = -1.3)),
  inhibitor_3pl = list(x = seq(-9, -3, 0.5), p = c(Bottom = 5, Top = 95, LogIC50 = -6)),
  inhibitor_4pl = list(x = seq(-9, -3, 0.5), p = c(Bottom = 5, Top = 95, LogIC50 = -6, HillSlope = -1.3)),
  inhibitor_5pl = list(x = seq(-9, -3, 0.25), p = c(Bottom = 5, Top = 95, LogIC50 = -6, HillSlope = -1.3, S = 0.6)),
  inhibitor_conc = list(x = c(0.1, 0.3, 1, 3, 10, 30, 100), p = c(Bottom = 2, Top = 50, IC50 = 3, HillSlope = 1.5)),
  one_phase_decay = list(x = 0:20, p = c(Y0 = 100, Plateau = 10, K = 0.3)),
  plateau_decay = list(x = seq(0, 20, 0.5), p = c(X0 = 2.2, Y0 = 100, Plateau = 10, K = 0.4)),
  two_phase_decay = list(x = seq(0, 40, 0.5), p = c(Y0 = 100, Plateau = 5, PercentFast = 60, KFast = 1, KSlow = 0.1)),
  one_phase_association = list(x = 0:20, p = c(Y0 = 5, Plateau = 80, K = 0.25)),
  exponential_growth = list(x = 0:15, p = c(Y0 = 2, K = 0.2)),
  one_site_specific = list(x = c(0.5, 1, 2, 4, 8, 16, 32, 64), p = c(Bmax = 100, Kd = 5)),
  one_site_total = list(x = c(0.5, 1, 2, 4, 8, 16, 32, 64), p = c(Bmax = 100, Kd = 5, NS = 0.5, Background = 3)),
  specific_hill = list(x = c(0.5, 1, 2, 4, 8, 16, 32, 64), p = c(Bmax = 100, Kd = 5, h = 1.4)),
  michaelis_menten = list(x = c(0.25, 0.5, 1, 2, 4, 8, 16), p = c(Vmax = 50, Km = 2)),
  substrate_inhibition = list(x = c(0.25, 0.5, 1, 2, 4, 8, 16, 32, 64, 128, 200), p = c(Vmax = 80, Km = 2, Ki = 20)),
  gaussian = list(x = seq(0, 10, 0.5), p = c(Amplitude = 10, Mean = 5, SD = 1.5))
)
missing <- setdiff(names(fit_models), names(truth))
if (length(missing) > 0) check(paste("models without a recovery check:", paste(missing, collapse = ", ")), 1, 0, 0)
for (name in names(truth)) {
  t <- truth[[name]]
  yy <- fit_models[[name]]$f(t$x, as.list(t$p))
  res <- tryCatch(op_curve_fit(list(x = t$x, y = yy), list(model = name)), error = function(e) {
    check(paste0(name, " fits (", conditionMessage(e), ")"), 1, 0, 0)
    NULL
  })
  if (is.null(res)) next
  got <- setNames(field(res$params, "value"), vapply(res$params, `[[`, "", "name"))
  check(paste(name, "recovers its parameters"), got[names(t$p)], t$p, 1e-4)
}

cat(if (failures == 0) "All checks passed\n" else sprintf("%d check(s) failed\n", failures))
quit(save = "no", status = if (failures == 0) 0 else 1)
