# cwm
cwm (pronounced "coom") - the CHAOTIC window manager

## testing
it is recommended that you use xephyr for testing the window manager,
as cwm is not yet ready to be used as a main window manager.
<br>
to test, first start up xephyr by running something like 
`Xephyr -ac -screen 1280x720 -br -reset -terminate 2 > /dev/null :1` 
and then use cargo to build and run cwm!