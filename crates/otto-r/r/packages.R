# Installs Otto's R packages into Otto's own library from a dated CRAN snapshot
# (prebuilt Windows binaries), so every Otto installation computes with the same versions.
#   Rscript --vanilla packages.R <package library> <repository> <package> ...
args <- commandArgs(trailingOnly = TRUE)
lib <- args[1]
repo <- args[2]
pkgs <- args[-(1:2)]
dir.create(lib, recursive = TRUE, showWarnings = FALSE)
.libPaths(lib)
options(repos = c(CRAN = repo), install.packages.check.source = "no")
cat("Installing into ", lib, " from ", repo, "\n", sep = "")
flush(stdout())
install.packages(pkgs, lib = lib, dependencies = c("Depends", "Imports", "LinkingTo"),
  type = if (.Platform$OS.type == "windows") "binary" else getOption("pkgType"))
found <- basename(find.package(pkgs, lib.loc = lib, quiet = TRUE))
missing <- setdiff(pkgs, found)
if (length(missing) > 0) {
  cat("@@otto-setup-failed ", paste(missing, collapse = ","), "\n", sep = "")
  quit(save = "no", status = 1)
}
cat("@@otto-setup-done\n")
