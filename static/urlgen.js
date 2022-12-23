(function() {
    function setup(id) {
        var inputEl = document.getElementById(id);
        var preEl = document.getElementById(id + "-url");
        function onChange(ev) {
            setTimeout(function() {
                var value = encodeURIComponent(inputEl.value.replace(/^#/, ""));
                preEl.innerText = value ?
                    "https://relay.fedi.buzz/" + id + "/" + value :
                    "";
            }, 10);
        }
        inputEl.addEventListener('change', onChange);
        inputEl.addEventListener('keyup', onChange);
    }

    setup("tag");
    setup("instance");
})()
