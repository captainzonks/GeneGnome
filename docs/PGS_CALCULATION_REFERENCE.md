# PGS Calculation Reference - Educational Attainment Project

**Project**: GeneGnome Educational Attainment Results Page
**Course**: PSYC 4152 - Behavioral Genetics
**Student**: Matthew Barham
**Due Date**: December 3, 2025
**Assignment**: Lab 13 - Return of Results Report (Final Project)

---

## Project Overview

This is a **final project** for PSYC 4152 demonstrating understanding of polygenic scores (PGS) and their interpretation. The project involves creating a web-based "return of results" page that explains genetic results to a hypothetical customer in accessible, educational language.

### Assignment Source
- Based on coursework for behavioral genetics laboratory
- Demonstrates practical application of PGS concepts
- Reference materials available in course documentation

### Key Objectives
1. Demonstrate understanding of PGS calculations and interpretation
2. Explain complex genetic concepts in accessible language
3. Correctly apply statistical concepts (R², correlation, z-scores)
4. Address limitations and caveats of genetic prediction
5. Create an educational, user-friendly interface

---

## Critical Conceptual Understanding: PGS vs Z-Score

### The Fundamental Misconception (AVOID THIS!)

**WRONG THINKING:**
- "PGS z-score = 3.5, so multiply by trait SD: 3.5 × 3 = 10.5 years boost"
- "Total education = 14 + 10.5 = 24.5 years" ❌

**WHY IT'S WRONG:**
The PGS z-score represents your *genetic* deviation from the population mean, but **genetics don't fully determine the trait**. You must account for R² (variance explained).

### The Correct Understanding

**PGS scores are already standardized (z-scores):**
- Mean = 0, SD = 1 in the reference population
- A PGS of 3.5 means you're 3.5 standard deviations above the *genetic* average
- But this doesn't directly translate to 3.5 SD above average in the *trait*

**The R² Connection:**
- R² = 0.0946 means genetics explain only ~9.5% of educational variance
- The other ~90.5% comes from environment, gene-environment interactions, measurement error, etc.
- Correlation (r) = √R² = √0.0946 = 0.3076

**The Correct Formula:**
```
Predicted trait z-score = correlation × PGS z-score
                        = r × PGS z-score
                        = 0.3076 × 3.5
                        = 1.076 SD
```

Then convert to years:
```
Predicted years = (predicted z-score × trait SD) + trait mean
                = (1.076 × 3) + 14
                = 3.23 + 14
                = 17.23 years
```

---

## Assignment Requirements (QUESTION 3)

### Given Information
From **Homework_Lab13.odt**, Question 3:

| Parameter | Value | Source |
|-----------|-------|--------|
| **PGS z-score** | 3.5 | Assignment prompt |
| **R²** | 0.0946 | PGS002012 (South Asian descent) |
| **PGS Identifier** | PGS002012 | PGS Catalog |
| **Trait mean (U.S.)** | 14 years | Assignment prompt |
| **Trait SD (U.S.)** | 3 years | Assignment prompt |
| **Customer** | 19-year-old female, South Asian descent | Assignment prompt |

### Important Note: PGS Version Confusion

**DO NOT CONFUSE:**
- **Lab 11 homework** used PGS002319 (R² = 0.0548) ← Different PGS!
- **Final project** uses PGS002012 (R² = 0.0946) ← Correct for this project!

Always verify which PGS version the assignment specifies!

---

## Step-by-Step Calculation (Correct Method)

### Step 1: Calculate Correlation from R²

```r
r = √R²
r = √0.0946
r = 0.3075711
r ≈ 0.3076
```

**Interpretation:** The correlation between the PGS and actual educational attainment is 0.31 (moderate-weak).

### Step 2: Calculate Predicted Trait Z-Score

```r
predicted_z = r × PGS_z
predicted_z = 0.3076 × 3.5
predicted_z = 1.076499
predicted_z ≈ 1.076
```

**Interpretation:** While your *genetics* place you 3.5 SD above average, your *predicted educational attainment* is only 1.076 SD above average because genetics only explain ~9.5% of the variance.

### Step 3: Convert Z-Score to Years

```r
predicted_years = (predicted_z × SD) + mean
predicted_years = (1.076 × 3) + 14
predicted_years = 3.228 + 14
predicted_years = 17.228
predicted_years ≈ 17.23 years
```

**Interpretation:** This corresponds to roughly a Master's degree level of education.

### R Code (From Homework Feedback)

```r
# Correct calculation from instructor feedback
(yed.cor <- sqrt(.0946))      # 0.3075711
(my.predicted.yed <- yed.cor * 3.5)  # 1.076499
my.predicted.yed * 3 + 14     # 17.2295 years
```

---

## Common Mistakes to Avoid

### ❌ Mistake 1: Direct Multiplication
```r
# WRONG!
predicted_years = PGS_z × SD + mean
                = 3.5 × 3 + 14
                = 24.5 years  # Incorrect!
```

**Why wrong:** Ignores that R² = 0.0946 (only 9.5% variance explained)

### ❌ Mistake 2: Using Wrong R²
```r
# WRONG PGS version!
r = √0.0548  # This is PGS002319 from Lab 11
r = 0.234

predicted_z = 0.234 × 3.5 = 0.819
predicted_years = (0.819 × 3) + 14 = 16.46 years  # Wrong!
```

**Why wrong:** Using PGS002319 instead of PGS002012

### ❌ Mistake 3: Confusing PGS with Trait Value
```
"Your PGS is 3.5, so you're 3.5 SD above average in education"
```

**Why wrong:** PGS measures *genetic* deviation, not trait deviation. Must apply correlation to get trait prediction.

---

## Research Findings: PGS Literature

### What is a Polygenic Score?

**From web research and course materials:**

1. **Raw PGS Construction:**
   ```
   Raw PGS = Σ(β_i × G_i)
   ```
   Where:
   - β_i = effect size (beta weight) from GWAS for SNP i
   - G_i = allele dosage (0, 1, or 2) for SNP i
   - Sum across all included SNPs (hundreds to millions)

2. **Standardization (Creating Z-Score):**
   ```
   Standardized PGS = (Raw PGS - mean_population) / SD_population
   ```
   Result: mean = 0, SD = 1

3. **Key Insight:**
   - In this class, ALL PGS scores are already standardized
   - When assignment says "PGS score of z=3.5", it's already a z-score
   - You're 3.5 SD above the *PGS mean*, not the *trait mean*

### Sources
- [Polygenic Score - Wikipedia](https://en.wikipedia.org/wiki/Polygenic_score)
- [PGS Catalog](https://www.pgscatalog.org/)
- [PGS in Biomedical Research - Nature Reviews Genetics](https://www.nature.com/articles/s41576-022-00470-z)
- [Guide to Performing PGS Analyses - PMC](https://pmc.ncbi.nlm.nih.gov/articles/PMC7612115/)

---

## File Structure & Locations

### Project Files

```
genegnome/
├── frontend/
│   └── www/
│       ├── results-example.html          # Main results page
│       ├── results-example.css           # Styling (741+ lines)
│       ├── results-example.js            # Chart.js visualizations
│       ├── darkmode.js                   # Dark mode toggle
│       └── themes.js                     # Theme management
├── docs/
│   └── PGS_CALCULATION_REFERENCE.md      # This file!
└── api-gateway/                          # Backend (Rust/Axum)
```

### Course Reference

This documentation is based on behavioral genetics coursework covering:
- R² calculations and variance explained
- PGS concepts and calculations
- Binary traits and odds ratio calculations

---

## Implementation Details

### HTML Structure (results-example.html)

**Key Sections:**
1. **Results Hero** (lines ~85-113): PGS z-score display
2. **Prediction Cards** (lines ~136-148): 17.23 years prediction
3. **Timeline Visual** (lines ~151-176): Education milestones
4. **Calculation Box** (lines ~242-272): Step-by-step math with MathJax
5. **Concept Cards** (lines ~196-340): Educational content
6. **Technical Details** (lines ~380-450): Study information, caveats

### MathJax Integration

**Added to `<head>`:**
```html
<script src="https://polyfill.io/v3/polyfill.min.js?features=es6"></script>
<script id="MathJax-script" async
  src="https://cdn.jsdelivr.net/npm/mathjax@3/es5/tex-mml-chtml.js">
</script>
```

**LaTeX Syntax in HTML:**
```html
<div class="step-math">
  \[r = \sqrt{R^2} = \sqrt{0.0946} = 0.3076\]
</div>
```

### CSS Styling (results-example.css)

**Calculation Box Styling** (lines 743-853):
- Gradient backgrounds with theme colors
- Hover animations (translateX on steps)
- Arrow indicators (→) using ::before pseudo-elements
- Dark mode support
- Responsive design for mobile
- MathJax font sizing adjustments

---

## Key Values Summary (Quick Reference)

| Variable | Value | Formula/Source |
|----------|-------|----------------|
| PGS z-score | 3.5 | Given (assignment) |
| R² | 0.0946 | PGS002012 catalog |
| Correlation (r) | 0.3076 | √0.0946 |
| Predicted trait z | 1.076 | 0.3076 × 3.5 |
| Predicted years | 17.23 | (1.076 × 3) + 14 |
| Education level | Master's | Interpretation |
| Percentile | 99.98% | pnorm(3.5) |

---

## Content Guidelines for Results Page

### Tone & Style
- **Accessible**: Explain technical terms in simple language
- **Educational**: Teach concepts, don't just report numbers
- **Empowering**: Genetics inform but don't determine
- **Honest**: Acknowledge limitations and uncertainties

### Must Include (Per Assignment)
1. ✅ Brief trait description (educational attainment)
2. ✅ PGS z-score explanation (3.5 SD above mean)
3. ✅ Study citation (APA format)
4. ✅ Percentile interpretation (99.98th percentile)
5. ✅ R² interpretation (9.5% variance explained)
6. ✅ Correlation meaning (0.31 = weak-moderate)
7. ✅ Baseline trait statistics (14 years mean, 3 SD)
8. ✅ Predicted trait value (17.23 years)
9. ✅ Caveats:
   - Genetics aren't destiny
   - This is average risk, not true risk
   - Environmental factors matter (genetic nurture, gene-environment correlation)

### Writing Tips
- Use **analogies** (e.g., "If genetics were sheet music...")
- Define **technical terms** inline (e.g., "phenotype (trait)")
- Include **visual aids** (charts, timelines)
- Show **calculations** step-by-step
- Address **common questions** proactively

---

## Common Questions & Answers

### Q: Why can't we just multiply PGS (3.5) by trait SD (3)?

**A:** Because the PGS z-score represents your *genetic* deviation, not your *trait* deviation. Genetics only explain R² = 9.5% of educational variance, so you must first multiply by the correlation (r = 0.31) to get your predicted *trait* z-score, which is only 1.076 SD.

### Q: What does R² = 0.0946 actually mean?

**A:** It means that 9.46% of the differences in educational attainment between people can be attributed to the genetic variants captured by this PGS. The other 90.54% is due to:
- Environmental factors (family SES, school quality, opportunities)
- Gene-environment interactions
- Gene-environment correlations (genetic nurture)
- Genetic variants not captured by the PGS
- Measurement error

### Q: Is 3.5 SD a normal PGS value?

**A:** No, it's very extreme! At pnorm(3.5) ≈ 99.98th percentile, only about 2 in 10,000 people have a PGS this high. This is an **example scenario** for educational purposes, not a typical real-world value.

### Q: Why use PGS002012 instead of PGS002319?

**A:** Different PGS versions exist because:
1. Different GWAS discovery samples (different ancestries, sample sizes)
2. Different PGS construction methods (clumping/thresholding, LDpred, etc.)
3. Different validation in target populations

PGS002012 has better performance (R² = 0.0946) in South Asian populations, which matches our hypothetical customer's ancestry.

---

## Development Status (as of 2025-12-01)

### ✅ Completed
- [x] Correct all numerical values (17.23 years, not 24.5)
- [x] Verify R² = 0.0946 (not 0.0548)
- [x] Verify PGS ID = PGS002012 (not PGS002319)
- [x] Add MathJax support for LaTeX rendering
- [x] Create calculation display box with step-by-step math
- [x] Style calculation box with gradients, hover effects, dark mode
- [x] Fix all infrastructure issues (502 errors, health checks)
- [x] Fix email template permissions (chmod 755)

### 🚧 In Progress
- [ ] Write DNA Fundamentals content (SNPs, variants)
- [ ] Write GWAS section content
- [ ] Fill remaining placeholder sections
- [ ] Verify all external references work
- [ ] Final polish and review

### 📅 Deadline
**December 3, 2025** - Final project due

---

## Lessons Learned

### 1. **Always Verify Assignment Parameters**
- Different labs use different PGS versions with different R² values
- Check the specific assignment prompt for exact values
- Don't assume continuity between assignments

### 2. **Correlation is Key**
- R² measures variance explained (0-100%)
- Correlation (r = √R²) is the slope for prediction (-1 to +1)
- Must apply correlation to convert genetic z-score to trait z-score

### 3. **Document Everything**
- Save assignment prompts for reference
- Keep calculation notes and R code
- Create reference documents (like this!) for complex topics

### 4. **Teaching Through Code**
- Visual math displays (MathJax) improve understanding
- Step-by-step breakdowns are more educational than final answers
- Explanations should accompany every calculation

---

## Quick Troubleshooting

### Issue: "Math formulas not rendering"
**Solution:** Check MathJax script loaded in `<head>`, verify \[...\] LaTeX syntax

### Issue: "Calculation box not styled"
**Solution:** Verify results-example.css loaded, check browser dev tools for CSS errors

### Issue: "Wrong predicted value (24.5 instead of 17.23)"
**Solution:** Verify using correlation: predicted_z = 0.3076 × 3.5 = 1.076, not PGS directly

### Issue: "Confused about which PGS to use"
**Solution:**
- Lab 11 homework → PGS002319 (R² = 0.0548)
- Final project → PGS002012 (R² = 0.0946)
- Always check assignment prompt!

---

## Additional Resources

### Course Materials
- Professor: Dr. Matthew Keller
- Course website: http://matthewckeller.com/
- PGS Catalog HTML: http://matthewckeller.com/html/scores.html
- R scripts: http://www.matthewckeller.com/R.Class/

### External References
- PGS Catalog: https://www.pgscatalog.org/
- PGS002012 page: https://www.pgscatalog.org/score/PGS002012/
- Genomics 101: https://www.genome.gov/
- Khan Academy Genetics: https://www.khanacademy.org/science/biology/classical-genetics

### Related Reading
- Harden, K. P. (2021). *The Genetic Lottery*. Princeton University Press.
- Lee, J. J., et al. (2018). Gene discovery and polygenic prediction from a genome-wide association study of educational attainment in 1.1 million individuals. *Nature Genetics*.

---

**Document Created:** 2025-12-01
**Last Updated:** 2025-12-01
**Author:** Claude Code (for Matthew Barham)
**Purpose:** Context preservation for future conversation sessions
