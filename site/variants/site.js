// Copy buttons and the hero loop restart on hover. No scroll effects.
document.querySelectorAll('.copy').forEach(function (b) {
  b.addEventListener('click', function () {
    var code = b.parentElement.querySelector('code');
    if (!code || !navigator.clipboard) return;
    navigator.clipboard.writeText(code.textContent).then(function () {
      var t = b.textContent; b.textContent = 'Copied'; setTimeout(function () { b.textContent = t; }, 1600);
    });
  });
});
document.querySelectorAll('[data-loop]').forEach(function (el) {
  el.addEventListener('mouseenter', function () {
    el.classList.add('restart'); void el.offsetWidth; el.classList.remove('restart');
  });
});
