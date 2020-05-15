pipeline {
  agent { label 'UbuntuVM' }
  parameters {
    gitParameter name: 'TAG', 
                 type: 'PT_TAG',
                 defaultValue: 'master'
  }

  stages {
    stage('Checkout Git TAG') {
      steps {
        cleanWs()
        checkout([$class: 'GitSCM',
                  branches: [[name: "${params.TAG}"]],
                  doGenerateSubmoduleConfigurations: false,
                  extensions: [],
                  gitTool: 'Default',
                  submoduleCfg: [],
                  userRemoteConfigs: [[url: 'https://github.com/atolab/eclipse-zenoh.git']]
                ])
      }
    }
    stage('Setup opam dependencies') {
      steps {
        sh '''
        git log --graph --date=short --pretty=tformat:'%ad - %h - %cn -%d %s' -n 20 || true
        OPAMJOBS=1 opam config report
        OPAMJOBS=1 opam install depext conf-libev
        OPAMJOBS=1 opam depext -yt
        OPAMJOBS=1 opam install -t . --deps-only
        '''
      }
    }
    stage('Build') {
      steps {
        sh '''
        opam exec -- dune build @all
        '''
      }
    }
    stage('Tests') {
      steps {
        sh '''
        opam exec -- dune runtest
        '''
      }
    }
    stage('Package') {
      steps {
        sh '''
        cp -r _build/default/install eclipse-zenoh
        tar czvf eclipse-zenoh-${TAG}-Ubuntu-20.04-x64.tgz eclipse-zenoh/*/*.*
        '''
      }
    }
    stage('Bocker build') {
      steps {
        sh '''
        docker build -t eclipse/zenoh:${TAG} .
        '''
      }
    }
  }

  post {
    success {
        archiveArtifacts artifacts: 'eclipse-zenoh-${TAG}-Ubuntu-20.04-x64.tgz', fingerprint: true
    }
  }
}
