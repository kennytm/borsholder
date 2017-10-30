'use strict';

function $(x) {
    return document.getElementById(x);
}
var $filter = $('filter');
var $prs = $('queue');
var $sort = $('sort');
$filter.onkeyup = $filter.onsearch = function() {
    var filterValue = $filter.value;
    try {
        var filter = new RegExp(filterValue, 'i');
        $filter.setCustomValidity('');
    } catch(e) {
        $filter.setCustomValidity(e);
        return;
    }
    var children = $prs.children;
    var filterCount = 0;
    for (var i = children.length - 1; i >= 0; -- i) {
        var li = children[i];
        if (filter.test(li.textContent.replace(/\s+/g, ' '))) {
            li.classList.remove('hidden');
            ++ filterCount;
        } else {
            li.classList.add('hidden');
        }
    }
    $('filter-status').innerHTML = filterValue && ('(' + filterCount + ' filtered)');
};
function updateSelectCount() {
    var allInputs = document.querySelectorAll('#queue input');
    var selectedCount = 0;
    for (var i = allInputs.length - 1; i >= 0; -- i) {
        selectedCount += allInputs[i].checked;
    }
    $('select-count').innerHTML = selectedCount;
}
function toggleCheckboxes(shouldChecked) {
    return function() {
        var children = $prs.children;
        var allInputs = document.querySelectorAll('#queue > li:not(.hidden) input');
        for (var i = allInputs.length - 1; i >= 0; -- i) {
            allInputs[i].checked = shouldChecked;
        }
        updateSelectCount();
    };
};
$('select').onclick = toggleCheckboxes(true);
$('deselect').onclick = toggleCheckboxes(false);
$prs.onclick = function(e) {
    if (e.target.tagName === 'INPUT') {
        updateSelectCount();
    }
};
var statusOrder = {
    'Pending': 0,
    'Approved': 1,
    'Error': 2,
    'Failure': 3,
    'Success': 4,
    'Reviewing': 5,
};
var sorters = {
    priority: function(a, b) {
        var aa = a.split(/:/);
        var bb = b.split(/:/);
        if (aa[0] !== bb[0]) {
            return statusOrder[aa[0]] - statusOrder[bb[0]];
        }
        if (aa[1] !== bb[1]) {
            return bb[1] - aa[1];
        }
        return aa[2] - bb[2];
    },
    number: function(a, b) {
        return b - a;
    },
};
function doSort() {
    var children = $prs.children;
    var array = new Array(children.length);
    var key = $sort.value;
    var sorter = sorters[key];
    for (var i = children.length - 1; i >= 0; -- i) {
        var li = children[i];
        array[i] = [li.dataset[key], li];
    }
    array.sort(function(a, b) {
        var aa = a[0];
        var bb = b[0];
        if (sorter) {
            return sorter(aa, bb);
        } else {
            return aa < bb ? 1 : aa > bb ? -1 : 0;
        }
    });
    for (var i = 0; i < array.length; ++ i) {
        var li = array[i][1];
        li.getElementsByClassName('order')[0].innerHTML = '#' + (i + 1);
        $prs.appendChild(li);
    }
};
$sort.onchange = doSort;
doSort();
$('rollup').onclick = function() {
    var allInputs = document.querySelectorAll('#queue input');
    var prs = [];
    for (var i = allInputs.length - 1; i >= 0; -- i) {
        var checkbox = allInputs[i];
        if (checkbox.checked) {
            prs.push(allInputs[i].parentNode.parentNode.parentNode.dataset.number|0);
        }
    }
    if (confirm('Create a rollup of ' + prs.length + ' PRs?')) {
        var state = encodeURIComponent(JSON.stringify({
            cmd: 'rollup',
            repo_label: '{{ args.homu_url | url_last_path_component }}',
            nums: prs,
        }));
        open('https://github.com/login/oauth/authorize?client_id={{ args.homu_client_id }}&scope=public_repo,admin:repo_hook&state=' + state);
    }
};

function adjustScrollPosition(e) {
    return function() {
        var comment = e.parentNode.getElementsByClassName('comment')[0];
        comment.scrollTop = comment.scrollHeight;
        e.onmouseenter = undefined;
    };
}

var commentMetadata = document.getElementsByClassName('comment-metadata');
for (var i = commentMetadata.length - 1; i >= 0; -- i) {
    var e = commentMetadata[i];
    e.onmouseenter = adjustScrollPosition(e);
}