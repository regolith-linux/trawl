project('demo', 'c',
  version : '0.1',
  default_options : ['warning_level=3'])

gio_dep = dependency('gio-unix-2.0')

lib_deps = [gio_dep]

sources = ['client_api.c']

sources += custom_target('dbus_codegen_files', 
  input: ['service.xml'],
  output: ['config_manager.c', 'config_manager.h'],
  command: ['scripts/run_codegen.sh', '@INPUT@', '@OUTPUT@'])


libclient_api = library ('regolith-config-client',
        sources,
        dependencies: lib_deps)

subdir('example')