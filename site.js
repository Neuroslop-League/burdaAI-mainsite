function savePreference(key, value) {
  try { localStorage.setItem(key, value); } catch {}
}

const root = document.documentElement;
const systemTheme = matchMedia('(prefers-color-scheme: dark)');
const theme = document.querySelector('#theme');

function syncTheme() {
  const dark = (root.dataset.theme ?? (systemTheme.matches ? 'dark' : 'light')) === 'dark';
  theme.setAttribute('aria-checked', String(dark));
}

theme.addEventListener('click', () => {
  root.dataset.theme = theme.getAttribute('aria-checked') === 'true' ? 'light' : 'dark';
  savePreference('burda-theme', root.dataset.theme);
  syncTheme();
});
systemTheme.addEventListener('change', syncTheme);
syncTheme();
theme.hidden = false;

const language = document.querySelector('#language');
const translations = Array.from(document.querySelectorAll('[data-en], [data-label-en]'), element => {
  const attribute = element.hasAttribute('data-label-en') ? 'aria-label' : null;
  return {
    element,
    attribute,
    ru: attribute ? element.getAttribute(attribute) : element.textContent,
    en: attribute ? element.dataset.labelEn : element.dataset.en,
  };
});
const description = document.querySelector('meta[name="description"]');
const russianDescription = description.content;

function setLanguage(value) {
  const english = value === 'en';
  root.lang = english ? 'en' : 'ru';
  for (const { element, attribute, ru, en } of translations) {
    if (attribute) element.setAttribute(attribute, english ? en : ru);
    else element.textContent = english ? en : ru;
  }
  language.textContent = english ? 'RU' : 'EN';
  language.lang = english ? 'ru' : 'en';
  language.setAttribute('aria-label', english ? 'Русский' : 'English');
  description.content = english
    ? 'burda.AI is a language model by Neuroslop League for local inference and reproducible training.'
    : russianDescription;
}

language.addEventListener('click', () => {
  setLanguage(root.lang === 'ru' ? 'en' : 'ru');
  savePreference('burda-language', root.lang);
});
let preferredLanguage;
try { preferredLanguage = localStorage.getItem('burda-language'); } catch {}
setLanguage(preferredLanguage);
language.hidden = false;

const filters = document.querySelector('.filters');
const filterButtons = Array.from(filters.querySelectorAll('button'));
const cards = document.querySelectorAll('.card');
for (const button of filterButtons) {
  button.addEventListener('click', () => {
    const topic = button.dataset.filter;
    for (const filter of filterButtons) filter.setAttribute('aria-pressed', String(filter === button));
    for (const card of cards) card.hidden = topic !== 'all' && card.dataset.topic !== topic;
  });
}
filters.hidden = false;

const palette = {
  ink: '#191917', night: '#21183d', dusk: '#30216d', blue: '#3328d4',
  violet: '#b8a5f9', purple: '#785fde', lime: '#d7fa3e', orange: '#ff733e',
  peach: '#ffb17e', cream: '#f5f3eb', green: '#879f29',
};

function paint(id, background, blocks) {
  const canvas = document.getElementById(id);
  const context = canvas.getContext('2d');
  if (!context) return;
  context.fillStyle = palette[background];
  context.fillRect(0, 0, canvas.width, canvas.height);
  for (const [color, x, y, width = 1, height = 1] of blocks) {
    context.fillStyle = palette[color];
    context.fillRect(x, y, width, height);
  }
}

paint('landscape', 'blue', [
  ['dusk', 0, 0, 48, 6], ['dusk', 0, 6, 48, 3],
  ['purple', 0, 20, 48, 5], ['violet', 0, 25, 48, 3],
  ['cream', 4, 5], ['purple', 9, 9, 2], ['violet', 15, 3], ['lime', 23, 6],
  ['violet', 43, 4], ['peach', 39, 15], ['violet', 5, 14], ['peach', 44, 18, 3],
  ['orange', 32, 5, 8, 1], ['orange', 30, 6, 12, 2], ['orange', 29, 8, 14, 6],
  ['orange', 30, 14, 12, 2], ['orange', 32, 16, 8, 1],
  ['peach', 30, 8, 12, 1], ['peach', 29, 11, 14, 1], ['peach', 30, 14, 12, 1],
  ['night', 0, 25, 48, 11],
  ['violet', 0, 22, 4, 3], ['violet', 4, 20, 3, 6], ['violet', 7, 18, 3, 9],
  ['violet', 10, 15, 3, 13], ['violet', 13, 12, 4, 16], ['violet', 17, 14, 3, 13],
  ['violet', 20, 17, 3, 10], ['violet', 23, 19, 3, 8], ['violet', 26, 22, 3, 5],
  ['cream', 13, 12, 4, 2], ['cream', 11, 14, 6, 2], ['cream', 10, 16, 3],
  ['purple', 16, 15, 4, 4], ['purple', 18, 19, 5, 3], ['purple', 20, 22, 6, 3],
  ['dusk', 37, 21, 3, 5], ['dusk', 40, 19, 5, 7], ['dusk', 45, 22, 3, 4],
  ['green', 0, 29, 10, 1], ['lime', 7, 28, 8, 1], ['lime', 14, 27, 5, 1],
  ['lime', 18, 28, 6, 1], ['lime', 23, 29, 10, 1], ['lime', 32, 28, 5, 1],
  ['lime', 36, 27, 5, 1], ['green', 40, 26, 8, 1],
  ['green', 25, 30, 6, 2], ['lime', 26, 32, 5, 1], ['lime', 29, 33, 9, 1],
  ['lime', 34, 34, 9, 2], ['purple', 7, 32, 2, 2], ['orange', 13, 34], ['violet', 45, 32, 3, 1],
]);

paint('model', 'violet', [
  ['purple', 4, 4, 7, 7], ['ink', 5, 3, 7, 7], ['cream', 6, 4, 5, 5], ['blue', 7, 5, 3, 3],
  ['ink', 12, 6, 5, 1], ['ink', 16, 6, 1, 6], ['ink', 16, 11, 6, 1],
  ['purple', 21, 8, 7, 7], ['ink', 22, 7, 7, 7], ['orange', 23, 8, 5, 5], ['lime', 24, 9, 3, 3],
  ['blue', 27, 2, 4, 3], ['cream', 2, 13, 3, 2], ['purple', 33, 11, 2, 2],
]);

paint('training', 'lime', [
  ['green', 6, 4, 24, 11], ['ink', 5, 3, 24, 11], ['night', 6, 4, 22, 9],
  ['violet', 8, 10, 2, 1], ['violet', 10, 9, 3, 1], ['violet', 13, 8, 3, 1],
  ['violet', 16, 9, 3, 1], ['violet', 19, 7, 3, 1], ['violet', 22, 6, 3, 1],
  ['orange', 25, 5, 2, 2], ['blue', 32, 4, 2, 2], ['cream', 2, 11, 2, 2],
]);

paint('runtime', 'orange', [
  ['peach', 6, 3, 24, 13], ['ink', 5, 2, 24, 13], ['blue', 6, 3, 22, 11],
  ['violet', 6, 3, 22, 2], ['ink', 8, 3], ['orange', 10, 3], ['lime', 12, 3],
  ['cream', 9, 7, 2, 1], ['cream', 11, 8, 2, 1], ['cream', 13, 9, 2, 1],
  ['cream', 11, 10, 2, 1], ['cream', 9, 11, 2, 1], ['lime', 17, 11, 6, 1],
  ['violet', 31, 10, 3, 2], ['ink', 2, 4, 2, 2],
]);
