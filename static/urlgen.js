(function() {
    function setup(id) {
        var inputEl = document.getElementById(id);
        var preEl = document.getElementById(id + "-url");
        function onChange(ev) {
            setTimeout(function() {
                var value = encodeURIComponent(inputEl.value.replace(/^#/, ""));
                preEl.innerText = value ?
                    "https://" + document.location.host + "/" + id + "/" + value :
                    "\n";
            }, 10);
        }
        inputEl.addEventListener('change', onChange);
        inputEl.addEventListener('keyup', onChange);
    }

    setup("tag");
    setup("instance");
})()
