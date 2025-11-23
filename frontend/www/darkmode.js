// ==============================================================================
// darkmode.js - Dark Mode Toggle Implementation
// ==============================================================================
// Description: Standalone dark mode toggle with localStorage persistence
// Author: Matt Barham
// Created: 2025-11-20
// Version: 1.0.0
// ==============================================================================

const darkModeToggle = document.getElementById('darkModeToggle');
if (darkModeToggle) {
    // Load saved preference
    const savedTheme = localStorage.getItem('theme');
    if (savedTheme === 'dark') {
        document.body.classList.add('dark-mode');
    }

    // Toggle on button click
    darkModeToggle.addEventListener('click', () => {
        document.body.classList.toggle('dark-mode');

        // Save preference
        if (document.body.classList.contains('dark-mode')) {
            localStorage.setItem('theme', 'dark');
        } else {
            localStorage.setItem('theme', 'light');
        }
    });
}
