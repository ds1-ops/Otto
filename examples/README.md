# A worked example

Seven synthetic experiments, the Otto project built from them, and what came out.
None of it is real measurement data: the numbers were generated to exercise every
graph type and every analysis Otto offers.

Open `Otto functional test.otto` to see the finished project, or import the CSVs
and rebuild it yourself.

## What is here

| Folder | Contents |
|---|---|
| `data/` | The seven input experiments, as they would come off a plate reader or an animal study |
| `output/` | What Otto exported: vector graphs (SVG), rasters (PNG), the plotted data (CSV) and equivalent ggplot2 / R code |
| `matplotlib/` | The same two graphs drawn in matplotlib, for comparison with Otto's publication format |

## The experiments

| File | Columns | Suits |
|---|---|---|
| `E1 cytokine release.csv` | Donor, Treatment, IL-6 (pg/mL) | Column bar, one-way ANOVA matched by donor |
| `E2 dose response.csv` | Cell line, log[Cmpd A] (nM), Replicate, Viability (%) | XY with a fitted dose-response curve |
| `E3 genotype x treatment.csv` | Genotype, Treatment, Mouse, Histology score | Grouped bars, two-way ANOVA |
| `E4 tumor growth.csv` | Group, Mouse, Day, Tumor volume (mm3) | XY over time, one line per animal |
| `E5 survival.csv` | Group, Mouse, Time (days), Event | Kaplan-Meier |
| `E6 target engagement.csv` | Animal, pS6 inhibition (%), Tumor change (%) | Scatter with correlation |
| `E7 qpcr panel.csv` | Gene, Condition, Replicate, log2 fold change | Heatmap |

The exported R script in `output/` had one line edited: Otto writes the absolute
path of its package library into exported code, so the copy here reads that path
from `LOCALAPPDATA` instead of naming one machine. Everything else is exactly what
Otto produced.

`E1`, `E3`, `E4` and `E5` measure the same subject more than once, so a graph or a
paired analysis needs that subject column (`Donor`, `Mouse`) set as **replicate by**
or **match by**. Without it nothing says which value belongs to which subject, and
one missing timepoint would shift every later value onto the wrong animal.
