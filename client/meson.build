project('demo', 'c',
  version : '0.1',
  default_options : ['warning_level=3'])

gio_dep = dependency('gio-unix-2.0')

sources = ['src/config_manager.c']
dependencies = [gio_dep]

library ('regolith-config-client',
        sources,
        include_directories: 'include',
        dependencies: dependencies)
