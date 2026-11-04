# Otto's statistics worker: reads one JSON request per line on stdin and answers each
# with one line on stdout, prefixed with "@@otto ". Started by Otto as
#   Rscript --vanilla worker.R <script dir> <package library>

args <- commandArgs(trailingOnly = TRUE)
otto_dir <- args[1]
otto_lib <- args[2]
# Only Otto's own library (pinned versions) plus R's base packages.
.libPaths(otto_lib)

required <- c("jsonlite", "car", "nortest", "minpack.lm", "survival")
found <- basename(find.package(required, quiet = TRUE))
missing <- setdiff(required, found)
cat("@@otto-hello R=", as.character(getRversion()), " missing=", paste(missing, collapse = ","), "\n", sep = "")
flush(stdout())
if (length(missing) > 0) quit(save = "no", status = 3)

for (f in c("common.R", "tests.R", "anova.R", "twoway.R", "normality.R", "outliers.R", "xy.R", "survival.R", "contingency.R")) source(file.path(otto_dir, f))

input <- file("stdin")
open(input)
repeat {
  line <- readLines(input, n = 1, warn = FALSE)
  if (length(line) == 0) break
  if (!nzchar(line)) next
  reply <- tryCatch({
    req <- parse_request(line)
    tryCatch(
      list(id = req$id, ok = TRUE, result = dispatch(req$op, req$data, req$options), engine = engine_info()),
      error = function(e) list(id = req$id, ok = FALSE, error = conditionMessage(e), engine = engine_info())
    )
  }, error = function(e) list(id = NULL, ok = FALSE, error = paste("bad request:", conditionMessage(e))))
  cat("@@otto ", to_json(reply), "\n", sep = "")
  flush(stdout())
}
