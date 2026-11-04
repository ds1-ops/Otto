# Otto graph "Genotype x treatment (final)" (Grouped (interleaved))
# Reproduces the graph from its plotted data: 45 x 30 mm axes, 1 pt lines, 7 pt Arial Bold.
# Packages: install.packages(c("ggplot2", "ggbeeswarm", "ggh4x", "svglite"))

library(ggplot2)
library(ggbeeswarm)
library(ggh4x)

# Plotted data: the rows used in the graph (after filters and missing values).
df <- read.csv(text = r"-(
Row,Genotype,Treatment,Histology score
1,WT,Vehicle,7.82
2,WT,Vehicle,7.96
3,WT,Vehicle,7.67
4,WT,Vehicle,7.32
5,WT,Vehicle,7.79
6,WT,Vehicle,7.9
7,WT,Vehicle,6.17
8,WT,Vehicle,5.8
9,WT,Cmpd A,1.44
10,WT,Cmpd A,4.04
11,WT,Cmpd A,3.54
12,WT,Cmpd A,2.8
13,WT,Cmpd A,4.77
14,WT,Cmpd A,2.99
15,WT,Cmpd A,3.28
16,WT,Cmpd A,3.36
17,Tnf-KO,Vehicle,3.95
18,Tnf-KO,Vehicle,1.22
19,Tnf-KO,Vehicle,2.93
20,Tnf-KO,Vehicle,2.86
21,Tnf-KO,Vehicle,4.24
22,Tnf-KO,Vehicle,3.29
23,Tnf-KO,Vehicle,5.97
24,Tnf-KO,Vehicle,4.07
25,Tnf-KO,Cmpd A,4.22
26,Tnf-KO,Cmpd A,3.83
27,Tnf-KO,Cmpd A,4.82
28,Tnf-KO,Cmpd A,5.09
29,Tnf-KO,Cmpd A,4.63
30,Tnf-KO,Cmpd A,4.43
31,Tnf-KO,Cmpd A,3.87
32,Tnf-KO,Cmpd A,3.05)-", check.names = FALSE,
               colClasses = c("Genotype" = "character", "Treatment" = "character"))
df[["Genotype"]] <- factor(df[["Genotype"]], levels = c("WT", "Tnf-KO"))
df[["Treatment"]] <- factor(df[["Treatment"]], levels = c("Vehicle", "Cmpd A"))

# Bar height / symbol = mean; error bars = SD.
otto_summary <- function(y) {
  m <- mean(y)
  data.frame(y = m, ymin = m - sd(y), ymax = m + sd(y))
}
lw <- 1 / (.pt * 72 / 96)  # 1 pt line width in ggplot2 units
pt_size <- 3.5 / .pt       # 3.5 pt symbol diameter

dodge <- position_dodge(width = 0.8)
p <- ggplot(df, aes(x = `Genotype`, y = `Histology score`, fill = `Treatment`, group = `Treatment`))
p <- p +
  stat_summary(fun = mean, geom = "col", position = dodge, width = 0.8, colour = "black", linewidth = lw) +
  geom_beeswarm(dodge.width = 0.8, size = pt_size, shape = 16, cex = 2.5) +
  stat_summary(fun.data = otto_summary, geom = "errorbar", position = dodge, width = 0.2, linewidth = lw) +
  scale_fill_manual(values = c("Vehicle" = "#F19C90", "Cmpd A" = "#9DDAE8")) +
  scale_x_discrete(expand = expansion(add = 0.5)) +
  scale_y_continuous(breaks = c(0, 5, 10), expand = expansion(0)) +
  coord_cartesian(ylim = c(0, 10), clip = "off") +
  labs(x = "Genotype", y = "Histology score") +
  theme_classic(base_size = 7, base_family = "Arial") +
  theme(
    text = element_text(face = "bold", colour = "black"),
    axis.text = element_text(face = "bold", colour = "black", size = 7),
    axis.title = element_text(face = "bold", size = 7),
    axis.line = element_line(linewidth = lw, colour = "black", lineend = "square"),
    axis.ticks = element_line(linewidth = lw, colour = "black"),
    axis.ticks.length = unit(3, "pt"),
    legend.position = "right",
    legend.title = element_blank(),
    legend.key.size = unit(6, "pt"),
    legend.text = element_text(face = "bold", size = 7),
    plot.background = element_blank(),
    panel.background = element_blank()
  ) +
  force_panelsizes(rows = unit(30, "mm"), cols = unit(45, "mm"))

# Significance shown on the Otto graph:
#   WT | Vehicle vs. WT | Cmpd A: ****

print(p)
ggsave("Genotype_x_treatment__final_.svg", p, device = svglite::svglite, width = 74, height = 45, units = "mm")
# ggsave("Genotype_x_treatment__final_.pdf", p, device = cairo_pdf, width = 74, height = 45, units = "mm")
