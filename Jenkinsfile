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
        checkout([$class: 'GitSCM',
                  branches: [[name: "${params.TAG}"]],
                  doGenerateSubmoduleConfigurations: false,
                  extensions: [],
                  gitTool: 'Default',
                  submoduleCfg: [],
                  userRemoteConfigs: [[url: 'https://github.com/atolab/eclipse-zenoh-c.git']]
                ])
      }
    }
    stage('Simple build') {
      steps {
        sh '''
        git log --graph --date=short --pretty=tformat:'%ad - %h - %cn -%d %s' -n 20 || true
        make all
        '''
      }
    }
    stage('Cross-platforms build') {
      steps {
        sh '''
        docker images || true
        make all-cross
        '''
      }
    }
  }

  post {
    success {
        archiveArtifacts artifacts: 'build/crossbuilds/*/*zenohc*.*', fingerprint: true
    }
  }
}
