// ==============================================================================
// themes.js - Global Color Theme Selector
// ==============================================================================
// Description: Theme selector dropdown for switching between color palettes
// Author: Matt Barham
// Created: 2025-11-27
// Version: 1.0.0
// ==============================================================================

// Initialize theme selector
document.addEventListener('DOMContentLoaded', function() {
    initializeThemeSelector();
});

function initializeThemeSelector() {
    const themeButton = document.getElementById('themeSelectorButton');
    const themeMenu = document.getElementById('themeSelectorMenu');
    const themeOptions = document.querySelectorAll('.theme-selector-option');

    if (!themeButton || !themeMenu) return;

    // Load saved theme or default to 'earth'
    const savedTheme = localStorage.getItem('genegnome-color-theme') || 'earth';
    applyTheme(savedTheme);

    // Toggle dropdown on button click
    themeButton.addEventListener('click', function(e) {
        e.stopPropagation();
        themeMenu.classList.toggle('active');
    });

    // Close dropdown when clicking outside
    document.addEventListener('click', function(e) {
        if (!themeButton.contains(e.target) && !themeMenu.contains(e.target)) {
            themeMenu.classList.remove('active');
        }
    });

    // Handle theme selection
    themeOptions.forEach(option => {
        option.addEventListener('click', function() {
            const themeName = this.getAttribute('data-theme');
            applyTheme(themeName);
            localStorage.setItem('genegnome-color-theme', themeName);
            themeMenu.classList.remove('active');
        });
    });
}

function applyTheme(themeName) {
    // Set theme attribute on body
    document.body.setAttribute('data-theme', themeName);

    // Update active state on theme options
    const themeOptions = document.querySelectorAll('.theme-selector-option');
    themeOptions.forEach(option => {
        if (option.getAttribute('data-theme') === themeName) {
            option.classList.add('active');
        } else {
            option.classList.remove('active');
        }
    });

    // Reinitialize charts if they exist (for results-example page)
    if (typeof reinitializeCharts === 'function') {
        reinitializeCharts();
    }
}
