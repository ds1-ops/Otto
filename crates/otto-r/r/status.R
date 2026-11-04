# Reports R's version and which of Otto's packages are installed (no packages needed).
#   Rscript --vanilla status.R <package library> <package> ...
args <- commandArgs(trailingOnly = TRUE)
lib <- args[1]
cat("R=", as.character(getRversion()), "\n", sep = "")
for (p in args[-1]) {
  v <- tryCatch(as.character(utils::packageVersion(p, lib.loc = c(lib, .Library))), error = function(e) "")
  cat(p, "=", v, "\n", sep = "")
}
