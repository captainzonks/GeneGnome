/**
 * ===========================================================================
 * RESULTS EXAMPLE PAGE - JAVASCRIPT
 * Interactive visualizations and expandable sections
 * ===========================================================================
 */

// Wait for DOM to load
document.addEventListener('DOMContentLoaded', function() {
    initializeCharts();
    initializeExpandableSections();
});

/**
 * ===========================================================================
 * CHART INITIALIZATION
 * ===========================================================================
 */

// Reinitialize charts when theme changes (called from themes.js)
function reinitializeCharts() {
    // Destroy existing charts if they exist
    Chart.helpers.each(Chart.instances, function(instance) {
        instance.destroy();
    });

    // Reinitialize with new theme colors
    initializeCharts();
}

function initializeCharts() {
    // Get color variables from CSS - must use body element where theme vars are defined
    const styles = getComputedStyle(document.body);
    const primaryColor = styles.getPropertyValue('--primary-color').trim();
    const secondaryColor = styles.getPropertyValue('--secondary-color').trim();
    const borderColor = styles.getPropertyValue('--border-color').trim();

    // Use high-contrast colors for charts based on dark mode
    const isDarkMode = document.body.classList.contains('dark-mode');
    const chartTextColor = isDarkMode ? '#f5ede3' : '#2a241f';
    const chartTextMuted = isDarkMode ? '#b8a898' : '#5a4f45';
    const chartGridColor = isDarkMode ? '#4a3f2f' : '#d9c9b8';

    // Chart 1: Distribution Curve
    createDistributionChart(primaryColor, chartTextColor, chartTextMuted, chartGridColor);

    // Chart 2: Variance Explained Pie Chart
    const cardBg = styles.getPropertyValue('--card-bg').trim();
    createVarianceChart(primaryColor, secondaryColor, chartTextColor, cardBg, isDarkMode);
}

/**
 * Distribution Curve with User Position
 */
function createDistributionChart(primaryColor, textColor, textMuted, borderColor) {
    const ctx = document.getElementById('distributionChart');
    if (!ctx) return;

    // Destroy existing chart if it exists (Chart.js v4)
    const existingChart = Chart.getChart(ctx);
    if (existingChart) {
        existingChart.destroy();
    }

    // Generate normal distribution data
    const mean = 0;
    const stdDev = 1;
    const userZScore = 3.5;

    // Generate x values from -4 to +4 standard deviations
    const dataPoints = [];
    for (let x = -4; x <= 4; x += 0.1) {
        // Normal distribution formula
        const y = (1 / (stdDev * Math.sqrt(2 * Math.PI))) *
                  Math.exp(-0.5 * Math.pow((x - mean) / stdDev, 2));
        dataPoints.push({x: x, y: y});
    }

    new Chart(ctx, {
        type: 'line',
        data: {
            datasets: [{
                label: 'Population Distribution',
                data: dataPoints,
                borderColor: primaryColor,
                backgroundColor: hexToRgba(primaryColor, 0.125),
                fill: true,
                tension: 0.4,
                pointRadius: 0
            }]
        },
        options: {
            responsive: true,
            maintainAspectRatio: true,
            aspectRatio: 2,
            plugins: {
                legend: {
                    display: false
                },
                tooltip: {
                    enabled: false
                },
                title: {
                    display: true,
                    text: 'Normal Distribution of Educational Attainment PGS',
                    color: textColor,
                    font: {
                        size: 14
                    }
                },
                annotation: {
                    annotations: {
                        userLine: {
                            type: 'line',
                            scaleID: 'x',
                            value: userZScore,
                            borderColor: primaryColor,
                            borderWidth: 3,
                            borderDash: [5, 5],
                            label: {
                                display: true,
                                content: 'You (z=3.5)',
                                position: 'start',
                                backgroundColor: primaryColor,
                                color: 'white',
                                font: {
                                    weight: 'bold'
                                }
                            }
                        },
                        meanLine: {
                            type: 'line',
                            scaleID: 'x',
                            value: 0,
                            borderColor: textMuted,
                            borderWidth: 2,
                            borderDash: [10, 5],
                            label: {
                                display: true,
                                content: 'Average (z=0)',
                                position: 'end',
                                backgroundColor: textMuted,
                                color: 'white'
                            }
                        }
                    }
                }
            },
            scales: {
                x: {
                    type: 'linear',
                    min: -4,
                    max: 4,
                    title: {
                        display: true,
                        text: 'Standard Deviations from Mean',
                        color: textColor
                    },
                    ticks: {
                        color: textMuted,
                        stepSize: 1
                    },
                    grid: {
                        color: hexToRgba(borderColor, 0.25)
                    }
                },
                y: {
                    title: {
                        display: true,
                        text: 'Probability Density',
                        color: textColor
                    },
                    ticks: {
                        color: textMuted,
                        display: false
                    },
                    grid: {
                        color: hexToRgba(borderColor, 0.25)
                    }
                }
            }
        }
    });
}

/**
 * Variance Explained Pie Chart
 */
function createVarianceChart(primaryColor, secondaryColor, textColor, cardBg, isDarkMode) {
    const ctx = document.getElementById('varianceChart');
    if (!ctx) return;

    // Destroy existing chart if it exists (Chart.js v4)
    const existingChart = Chart.getChart(ctx);
    if (existingChart) {
        existingChart.destroy();
    }

    // Debug: Log colors
    console.log('Pie chart colors:', { primaryColor, secondaryColor, cardBg, textColor, isDarkMode });

    // Use theme colors with fallback
    const geneticColor = primaryColor || '#c1744a';
    const environmentColor = secondaryColor || '#7a8450';

    new Chart(ctx, {
        type: 'doughnut',
        data: {
            labels: ['Genetic (PGS)', 'Environment & Other'],
            datasets: [{
                data: [9.5, 90.5],
                backgroundColor: [geneticColor, environmentColor],
                borderColor: cardBg,
                borderWidth: 4
            }]
        },
        options: {
            responsive: true,
            maintainAspectRatio: true,
            aspectRatio: 1.5,
            plugins: {
                legend: {
                    position: 'bottom',
                    labels: {
                        color: textColor,
                        padding: 15,
                        font: {
                            size: 12
                        }
                    }
                },
                tooltip: {
                    callbacks: {
                        label: function(context) {
                            return context.label + ': ' + context.parsed + '%';
                        }
                    }
                },
                title: {
                    display: true,
                    text: 'What Explains Educational Attainment?',
                    color: textColor,
                    font: {
                        size: 14,
                        weight: 'bold'
                    },
                    padding: {
                        bottom: 15
                    }
                }
            }
        }
    });
}

/**
 * ===========================================================================
 * EXPANDABLE SECTIONS
 * ===========================================================================
 */

function initializeExpandableSections() {
    // Concept Cards
    initializeExpandables('.concept-card', '.concept-header');

    // Detail Cards (Scientific Details)
    initializeExpandables('.detail-card', '.detail-header');

    // FAQ Items
    initializeExpandables('.faq-item', '.faq-question');
}

function initializeExpandables(cardSelector, headerSelector) {
    const cards = document.querySelectorAll(cardSelector);

    cards.forEach(card => {
        const header = card.querySelector(headerSelector);
        if (!header) return;

        header.addEventListener('click', function() {
            // Close others in same section (accordion behavior)
            const parent = card.parentElement;
            const siblings = parent.querySelectorAll(cardSelector);
            siblings.forEach(sibling => {
                if (sibling !== card && sibling.classList.contains('expanded')) {
                    sibling.classList.remove('expanded');
                }
            });

            // Toggle this card
            card.classList.toggle('expanded');
        });
    });
}

/**
 * ===========================================================================
 * UTILITY FUNCTIONS
 * ===========================================================================
 */

/**
 * Convert hex color to rgba with alpha transparency
 * @param {string} hex - Hex color (e.g., '#c1744a' or 'c1744a')
 * @param {number} alpha - Alpha value 0-1 (e.g., 0.125 for light transparency)
 * @returns {string} rgba color string
 */
function hexToRgba(hex, alpha) {
    // Validate input
    if (!hex || typeof hex !== 'string') {
        console.warn('Invalid hex color:', hex);
        return `rgba(193, 116, 74, ${alpha})`; // Fallback to primary color
    }

    // Remove # if present and trim whitespace
    hex = hex.trim().replace('#', '');

    // Handle short hex format (#RGB -> #RRGGBB)
    if (hex.length === 3) {
        hex = hex.split('').map(char => char + char).join('');
    }

    // Validate hex format
    if (!/^[0-9A-Fa-f]{6}$/.test(hex)) {
        console.warn('Invalid hex format:', hex);
        return `rgba(193, 116, 74, ${alpha})`; // Fallback to primary color
    }

    // Parse RGB values
    const r = parseInt(hex.substring(0, 2), 16);
    const g = parseInt(hex.substring(2, 4), 16);
    const b = parseInt(hex.substring(4, 6), 16);

    // Validate parsed values
    if (isNaN(r) || isNaN(g) || isNaN(b)) {
        console.warn('Failed to parse hex color:', hex);
        return `rgba(193, 116, 74, ${alpha})`; // Fallback to primary color
    }

    return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

// Dark mode is handled globally by darkmode.js
// Charts will update automatically via CSS variables
