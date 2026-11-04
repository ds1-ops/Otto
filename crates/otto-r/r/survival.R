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
