set_policy("package.install_locally", true)

add_requires("opencore-amr", {system = false})
add_requires("libtool", {system = false})
add_requires("g7221", {system = false})

package("g7221")
    add_urls("https://github.com/freeswitch/libg7221.git")
    add_deps("libtool")
    on_install("linux", "macosx", "android", "iphoneos", "bsd", "cross", "mingw", function (package)
        local configs = {}
        table.insert(configs, "--enable-shared=" .. (package:config("shared") and "yes" or "no"))
        if package:is_debug() then
            table.insert(configs, "--enable-debug")
        end
        import("package.tools.autoconf").install(package, configs)
    end)

target("symphonia-voip")
    set_kind("phony")
    add_packages("opencore-amr")
    add_packages("g7221")
    before_build(function (target)
        local thirdir = "$(buildir)/3rd"
        local includedir = path.join(thirdir, "include")
        local libdir = path.join(thirdir, "lib")
        os.mkdir(includedir)
        os.mkdir(libdir)
        for _, pkg in pairs(target:pkgs()) do
            local installdir = pkg:installdir()
            os.cp(path.join(installdir, "include", "*"), includedir)
            os.cp(path.join(installdir, "lib", "*"), libdir)
        end
    end)
