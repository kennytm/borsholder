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
        var filter = new RegExp(filterValue, 'im');
        $filter.setCustomValidity('');
    } catch(e) {
        $filter.setCustomValidity(e);
        return;
    }
    var children = $prs.children;
    var filterCount = 0;
    for (var i = children.length - 1; i >= 0; -- i) {
        var li = children[i];
        if (filter.test(li.dataset.filter)) {
            li.classList.remove('hidden');
            ++ filterCount;
        } else {
            li.classList.add('hidden');
        }
    }
    $('filter-status').innerHTML = filterValue && ('(' + filterCount + ' filtered)');
};
function updateSelectCount() {
    var allInputs = document.querySelectorAll('#queue .number input');
    var selectedCount = 0;
    for (var i = allInputs.length - 1; i >= 0; -- i) {
        selectedCount += allInputs[i].checked;
    }
    $('select-count').innerHTML = selectedCount;
}
function toggleCheckboxes(shouldChecked) {
    return function() {
        var children = $prs.children;
        var allInputs = document.querySelectorAll('#queue > li:not(.hidden) > .number input');
        for (var i = allInputs.length - 1; i >= 0; -- i) {
            allInputs[i].checked = shouldChecked;
        }
        updateSelectCount();
    };
};
$('select').onclick = toggleCheckboxes(true);
$('deselect').onclick = toggleCheckboxes(false);
$prs.onclick = function(e) {
    var target = e.target;
    if (target.tagName === 'INPUT') {
        updateSelectCount();
    }


    if (HAS_ACTIVE_COMMENTS) {
        while (target) {
            if (target.className === 'comment') {
                return;
            }
            target = target.parentNode;
        }
        hideActiveComments();
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
    complexity: function(a, b) {
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
    var allInputs = document.querySelectorAll('#queue .number input');
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
            repo_label: HOMU_URL,
            nums: prs,
        }));
        open('https://github.com/login/oauth/authorize?client_id=' + CLIENT_ID + '&scope=public_repo,admin:repo_hook&state=' + state);
    }
};

function adjustScrollPosition(comment) {
    return function() {
        comment.scrollTop = comment.scrollHeight;
    };
}

var HAS_ACTIVE_COMMENTS = false;
function toggleVisibility(comment) {
    return function(e) {
        e.stopPropagation();
        if (HAS_ACTIVE_COMMENTS) {
            hideActiveComments();
        }
        HAS_ACTIVE_COMMENTS = true;
        comment.style.display = 'block';
    };
}
function hideActiveComments() {
    var allComments = document.getElementsByClassName('comment');
    for (var i = allComments.length - 1; i >= 0; -- i) {
        allComments[i].style.display = '';
    }
    HAS_ACTIVE_COMMENTS = false;
}

function recomputeRelativeTime() {
    var timeElems = document.getElementsByTagName('TIME');
    var now = Date.now();
    for (var i = timeElems.length - 1; i >= 0; -- i) {
        var elem = timeElems[i];
        var text;
        var dt = Date.parse(elem.getAttribute('datetime'));
        if (dt > 0) {
            var diff = ((now - dt) / 60000)|0;
            if (diff < 1) {
                text = '1 minute';
            } else if (diff < 60) {
                text = diff + ' minutes';
            } else if (diff < 3*60) {
                var hours = (diff / 60)|0;
                var minutes = diff % 60;
                text = hours + ' hour';
                if (hours > 1) {
                    text += 's';
                }
                if (minutes > 0) {
                    text += ' ' + minutes + ' minute';
                    if (minutes > 1) {
                        text += 's';
                    }
                }
            } else if (diff < 24*60) {
                var hours = (diff / 60)|0;
                text = hours + ' hours';
            } else {
                var days = (diff / (24*60))|0;
                text = days + ' day';
                if (days > 1) {
                    text += 's';
                }
            }
            text += ' ago';
        } else {
            text = 'at unknown time'
        }
        elem.innerHTML = text;
    }
}
setInterval(recomputeRelativeTime, 30000);
recomputeRelativeTime();

var commentMetadata = document.getElementsByClassName('comment-metadata');
for (var i = commentMetadata.length - 1; i >= 0; -- i) {
    var e = commentMetadata[i];
    var comment = e.parentNode.getElementsByClassName('comment')[0];
    e.onmouseenter = adjustScrollPosition(comment);
    e.onclick = toggleVisibility(comment);
}

document.body.removeChild($('loading-text'));
